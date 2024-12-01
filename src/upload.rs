use crate::config::Config;
use crate::mqtt_service::{FileDetail, MqttService, UploadRequest};
use crate::progress_tracker::{ProgressTracker, SharedState};
use crate::service_utils::{publish_analytics, publish_progress, publish_status, start_logging};
use log::{error, info, warn};
use std::error::Error;
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

use async_compression::tokio::bufread::GzipEncoder;
use async_compression::Level;
use ssh2::Session;

const CHUNK_SIZE: usize = 8 * 1024; // 8 KB

pub async fn process_and_upload(
    tracker: Arc<ProgressTracker>,
    upload_request: UploadRequest,
    config: Config,
    _state: SharedState,
    mqtt_service: Arc<MqttService>, // Added mqtt_service as a parameter
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Start logging the upload process
    start_logging(
        mqtt_service.clone(),
        format!("Starting upload task {}", tracker.task_id),
    );

    // Publish status update
    publish_status(
        mqtt_service.clone(),
        "upload_started".to_string(),
        Some(format!("Upload task {} has started.", tracker.task_id)),
    );

    let files_to_upload = collect_files(&upload_request, &config).await?;

    let total_size = estimate_total_size(&files_to_upload).await?;
    tracker.set_total_size(total_size).await;

    // Publish analytics event
    publish_analytics(
        mqtt_service.clone(),
        "files_collected".to_string(),
        format!(
            "Collected {} files totaling {} bytes for upload task {}.",
            files_to_upload.len(),
            total_size,
            tracker.task_id
        ),
    );

    // Adjust upload strategy
    match config.upload_strategy.as_str() {
        "batch" => {
            let semaphore = Arc::new(Semaphore::new(5)); // Number of concurrent uploads
            let mut tasks = Vec::new();

            for file_detail in files_to_upload {
                let permit = semaphore.clone().acquire_owned().await?;
                let tracker = tracker.clone();
                let config = config.clone();
                let upload_request = upload_request.clone();
                let mqtt_service = mqtt_service.clone();

                tasks.push(tokio::spawn(async move {
                    let result = upload_single_file(
                        file_detail,
                        &upload_request,
                        &config,
                        tracker,
                        mqtt_service,
                    )
                        .await;
                    drop(permit); // Release the semaphore
                    result
                }));
            }

            // Wait for all uploads
            for task in tasks {
                if let Err(e) = task.await.unwrap_or_else(|_| Err("Task panicked".into())) {
                    error!("Error during upload: {:?}", e);

                    // Log error
                    start_logging(
                        mqtt_service.clone(),
                        format!("Error during upload: {:?}", e),
                    );

                    // Publish status update
                    publish_status(
                        mqtt_service.clone(),
                        "upload_error".to_string(),
                        Some(format!("Error during upload task {}.", tracker.task_id)),
                    );
                }
            }
        }
        "sequential" => {
            for file_detail in files_to_upload {
                if let Err(e) = upload_single_file(
                    file_detail,
                    &upload_request,
                    &config,
                    tracker.clone(),
                    mqtt_service.clone(),
                )
                    .await
                {
                    error!("Error during upload: {:?}", e);

                    // Log error
                    start_logging(
                        mqtt_service.clone(),
                        format!("Error during upload: {:?}", e),
                    );

                    // Publish status update
                    publish_status(
                        mqtt_service.clone(),
                        "upload_error".to_string(),
                        Some(format!("Error during upload task {}.", tracker.task_id)),
                    );
                }
            }
        }
        _ => return Err("Unsupported upload strategy".into()),
    }

    info!("Upload successfully completed.");

    // Publish status update
    publish_status(
        mqtt_service.clone(),
        "upload_completed".to_string(),
        Some(format!("Upload task {} completed successfully.", tracker.task_id)),
    );

    // Publish analytics event
    publish_analytics(
        mqtt_service.clone(),
        "upload_complete".to_string(),
        format!("Upload task {} completed successfully.", tracker.task_id),
    );

    // Log completion
    start_logging(
        mqtt_service.clone(),
        format!("Upload task {} completed successfully.", tracker.task_id),
    );

    Ok(())
}

async fn collect_files(
    upload_request: &UploadRequest,
    config: &Config,
) -> Result<Vec<FileDetail>, Box<dyn Error + Send + Sync>> {
    let files = if upload_request.recursive.unwrap_or(config.recursive_upload) {
        let root_folder = upload_request
            .root_folder
            .as_deref()
            .unwrap_or(&config.default_file_source);
        let file_filters = upload_request
            .file_filters
            .as_ref()
            .unwrap_or(&config.file_filters);
        collect_files_recursively(root_folder, file_filters).await?
    } else if let Some(ref files) = upload_request.files {
        files.clone()
    } else {
        return Err("No files provided for upload.".into());
    };

    // Apply file size filter asynchronously
    let max_size_bytes = (config.max_file_upload_size_mb * 1024 * 1024) as u64;
    let mut valid_files = Vec::new();

    for file in files {
        if let Ok(metadata) = tokio::fs::metadata(&file.source_path).await {
            if metadata.len() <= max_size_bytes {
                valid_files.push(file);
            } else {
                warn!(
                    "File {} exceeds the maximum size limit of {} MB and will be skipped.",
                    file.source_path, config.max_file_upload_size_mb
                );
            }
        } else {
            warn!("Failed to read metadata for file: {}", file.source_path);
        }
    }

    Ok(valid_files)
}

fn collect_files_recursively<'a>(
    root_folder: &'a str,
    file_filters: &'a [String],
) -> Pin<
    Box<dyn Future<Output = Result<Vec<FileDetail>, Box<dyn Error + Send + Sync>>> + Send + 'a>,
> {
    Box::pin(async move {
        let mut files = Vec::new();
        let mut dir_entries = tokio::fs::read_dir(root_folder).await?;

        while let Some(entry) = dir_entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let sub_files = collect_files_recursively(
                    path.to_str().unwrap_or_default(),
                    file_filters,
                )
                    .await?;
                files.extend(sub_files);
            } else if let Some(extension) = path.extension() {
                if file_filters.contains(&extension.to_string_lossy().to_lowercase()) {
                    files.push(FileDetail {
                        source_path: path.to_str().unwrap_or_default().to_string(),
                        destination_path: path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .to_string(),
                    });
                }
            }
        }
        Ok(files)
    })
}

async fn estimate_total_size(
    files: &[FileDetail],
) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let mut total_size = 0u64;
    for file in files {
        total_size += tokio::fs::metadata(&file.source_path).await?.len();
    }
    Ok(total_size)
}

async fn upload_single_file(
    file_detail: FileDetail,
    upload_request: &UploadRequest,
    config: &Config,
    tracker: Arc<ProgressTracker>,
    mqtt_service: Arc<MqttService>, // Added mqtt_service as a parameter
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Start logging
    start_logging(
        mqtt_service.clone(),
        format!("Starting upload of file {}", file_detail.source_path),
    );

    let file = File::open(&file_detail.source_path).await?;
    let reader: Box<dyn io::AsyncRead + Unpin + Send> = if upload_request
        .compression
        .as_ref()
        .map(|c| c.enabled)
        .unwrap_or(config.allow_compression)
    {
        Box::new(compress_stream(file, config.compression_quality).await?)
    } else {
        Box::new(BufReader::new(file))
    };

    match upload_request.upload_type.as_str() {
        "smb" => {
            smb_upload(
                reader,
                &file_detail.destination_path,
                config,
                tracker.clone(),
                mqtt_service.clone(),
            )
                .await?
        }
        "sftp" => {
            sftp_upload(
                reader,
                &file_detail.destination_path,
                config,
                tracker.clone(),
                mqtt_service.clone(),
            )
                .await?
        }
        _ => return Err("Unsupported upload type".into()),
    }

    // Publish analytics event
    publish_analytics(
        mqtt_service.clone(),
        "file_uploaded".to_string(),
        format!("File {} uploaded successfully.", file_detail.source_path),
    );

    // Log completion
    start_logging(
        mqtt_service.clone(),
        format!("File {} uploaded successfully.", file_detail.source_path),
    );

    Ok(())
}

async fn smb_upload<R>(
    mut reader: R,
    destination_path: &str,
    config: &Config,
    tracker: Arc<ProgressTracker>,
    mqtt_service: Arc<MqttService>, // Added mqtt_service as a parameter
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: io::AsyncRead + Unpin + Send,
{
    use std::process::Stdio;
    use tokio::process::Command;

    let full_destination_path = format!("{}/{}", config.smb_target_folder, destination_path);

    // Add timeout to the smbclient command
    let smb_command = format!(
        "timeout {} smbclient //{}/{} -U {}%{} -c 'put - \"{}\"'",
        config.smb_connection_timeout_ms / 1000, // Convert ms to seconds for the timeout command
        config.smb_target_ip,
        config.smb_share_name,
        config.smb_username,
        config.smb_password,
        full_destination_path
    );

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&smb_command)
        .stdin(Stdio::piped())
        .spawn()?;

    let mut stdin = BufWriter::new(child.stdin.take().ok_or("Failed to open stdin")?);
    let mut buffer = [0; CHUNK_SIZE];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        stdin.write_all(&buffer[..n]).await?;
        tracker.update_progress(n as u64).await;

        // Publish progress
        publish_progress(
            mqtt_service.clone(),
            *tracker.uploaded_size.lock().await,
            *tracker.total_size.lock().await,
        );
    }
    stdin.flush().await?;
    let status = child.wait().await?;
    if !status.success() {
        return Err("smbclient command failed".into());
    }

    Ok(())
}

async fn sftp_upload<R>(
    mut reader: R,
    destination_path: &str,
    config: &Config,
    tracker: Arc<ProgressTracker>,
    mqtt_service: Arc<MqttService>, // Added mqtt_service as a parameter
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: io::AsyncRead + Unpin + Send,
{
    // Apply timeout to the TCP connection
    let tcp = tokio::time::timeout(
        Duration::from_millis(config.sftp_connection_timeout_ms),
        TcpStream::connect(format!("{}:{}", config.sftp_host, config.sftp_port)),
    )
        .await
        .map_err(|_| "SFTP connection timed out")??; // Handle timeout error explicitly

    let tcp = tcp.into_std()?;
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;
    session.userauth_password(&config.sftp_username, &config.sftp_password)?;

    let sftp = session.sftp()?;

    // Construct the full path using `sftp_target_folder`
    let full_destination_path = format!("{}/{}", config.sftp_target_folder, destination_path);

    let mut remote_file = sftp.create(Path::new(&full_destination_path))?;
    let mut buffer = [0; CHUNK_SIZE];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        remote_file.write_all(&buffer[..n])?;
        tracker.update_progress(n as u64).await;

        // Publish progress
        publish_progress(
            mqtt_service.clone(),
            *tracker.uploaded_size.lock().await,
            *tracker.total_size.lock().await,
        );
    }

    Ok(())
}

async fn compress_stream(
    file: File,
    quality: u8,
) -> Result<GzipEncoder<BufReader<File>>, Box<dyn Error + Send + Sync>> {
    let level = match quality {
        0..=9 => Level::Precise(quality as i32),
        _ => Level::Default,
    };

    Ok(GzipEncoder::with_quality(BufReader::new(file), level))
}
