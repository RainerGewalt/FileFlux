use crate::config::Config;
use crate::mqtt_service::{FileDetail, FolderConfig, CompressionConfig, UploadRequest, MqttService};
use crate::progress_tracker::{ProgressTracker, SharedState};
use crate::service_utils::{publish_analytics, publish_progress, publish_status, start_logging};
use log::{error, info, warn};
use serde_json::json;
use std::error::Error;
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    mqtt_service: Arc<MqttService>,
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

    info!("Received upload request: {:?}", upload_request);

    let start_time = Instant::now();
    let files_to_upload = collect_files(&upload_request, &config).await?;
    let total_size = estimate_total_size(&files_to_upload).await?;
    tracker.set_total_size(total_size).await;

    // Publish analytics event: Anzahl Dateien
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

    let upload_strategy = upload_request
        .options
        .upload_strategy
        .as_deref()
        .unwrap_or(config.upload_strategy.as_str());

    let mut successful_uploads = 0;
    let mut failed_uploads = 0;

    match upload_strategy {
        "batch" => {
            let semaphore = Arc::new(Semaphore::new(5));
            let mut tasks = Vec::new();

            for file_detail in &files_to_upload {
                let permit = semaphore.clone().acquire_owned().await?;
                let tracker = tracker.clone();
                let config = config.clone();
                let upload_request = upload_request.clone();
                let mqtt_service = mqtt_service.clone();
                let file_detail = file_detail.clone();

                tasks.push(tokio::spawn(async move {
                    let result = upload_single_file(
                        file_detail,
                        &upload_request,
                        &config,
                        tracker,
                        mqtt_service,
                    ).await;
                    drop(permit);
                    result
                }));
            }

            for task in tasks {
                match task.await {
                    Ok(Ok(_)) => successful_uploads += 1,
                    Ok(Err(e)) => {
                        error!("Error during upload: {:?}", e);
                        failed_uploads += 1;
                    }
                    Err(_) => {
                        error!("Task panicked");
                        failed_uploads += 1;
                    }
                }
            }
        }
        "sequential" => {
            for file_detail in &files_to_upload {
                match upload_single_file(
                    file_detail.clone(),
                    &upload_request,
                    &config,
                    tracker.clone(),
                    mqtt_service.clone(),
                ).await {
                    Ok(_) => successful_uploads += 1,
                    Err(e) => {
                        error!("Error during upload: {:?}", e);
                        failed_uploads += 1;
                    }
                }
            }
        }
        _ => return Err("Unsupported upload strategy".into()),
    }

    if failed_uploads > 0 {
        start_logging(
            mqtt_service.clone(),
            format!("{} uploads failed during task {}", failed_uploads, tracker.task_id),
        );

        publish_status(
            mqtt_service.clone(),
            "upload_error".to_string(),
            Some(format!("{} uploads failed during upload task {}.", failed_uploads, tracker.task_id)),
        );
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let upload_speed_mb_s = if elapsed > 0.0 {
        (total_size as f64 / (1024.0 * 1024.0)) / elapsed
    } else {
        0.0
    };

    // Zusätzliche KPIs berechnen
    let total_files = files_to_upload.len();
    let success_rate = if total_files > 0 {
        (successful_uploads as f64 / total_files as f64) * 100.0
    } else {
        0.0
    };

    // Größen für min/max/average berechnen
    let mut sizes = Vec::with_capacity(total_files);
    for f in &files_to_upload {
        if let Ok(metadata) = tokio::fs::metadata(&f.source_path).await {
            sizes.push(metadata.len());
        }
    }

    let (average_file_size_mb, largest_file_mb, smallest_file_mb) = if !sizes.is_empty() {
        let sum: u64 = sizes.iter().sum();
        let avg = sum as f64 / sizes.len() as f64;
        let max_size = sizes.iter().max().cloned().unwrap_or(0);
        let min_size = sizes.iter().min().cloned().unwrap_or(0);

        (
            avg / (1024.0 * 1024.0),
            max_size as f64 / (1024.0 * 1024.0),
            min_size as f64 / (1024.0 * 1024.0),
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    // JSON-Payload für KPIs
    let kpi_json = json!({
        "totalFiles": total_files,
        "uploadedFiles": successful_uploads,
        "uploadSpeed": upload_speed_mb_s,
        "successfulUploads": successful_uploads,
        "failedUploads": failed_uploads,
        "successRate": success_rate,
        "averageFileSizeMB": average_file_size_mb,
        "largestFileMB": largest_file_mb,
        "smallestFileMB": smallest_file_mb,
        "elapsedSeconds": elapsed
    });

    // KPIs publizieren
    publish_analytics(
        mqtt_service.clone(),
        "upload_kpis".to_string(),
        kpi_json.to_string(),
    );

    // Abschließende Meldungen
    publish_analytics(
        mqtt_service.clone(),
        "upload_complete".to_string(),
        format!("Upload task {} completed successfully.", tracker.task_id),
    );

    start_logging(
        mqtt_service.clone(),
        format!("Upload task {} completed successfully.", tracker.task_id),
    );

    info!("Upload successfully completed.");

    publish_status(
        mqtt_service.clone(),
        "upload_completed".to_string(),
        Some(format!("Upload task {} completed successfully.", tracker.task_id)),
    );

    Ok(())
}

async fn collect_files(
    upload_request: &UploadRequest,
    config: &Config,
) -> Result<Vec<FileDetail>, Box<dyn Error + Send + Sync>> {
    let options = &upload_request.options;
    let mut files = Vec::new();

    if let Some(ref recursive_folders) = options.recursive_folders {
        for folder in recursive_folders {
            let folder_files = collect_files_recursively(
                &folder.path,
                &options
                    .file_filters
                    .clone()
                    .unwrap_or_else(|| config.file_filters.clone())
            ).await?;
            files.extend(folder_files);
        }
    }

    if let Some(ref specified_files) = options.files {
        files.extend(specified_files.clone());
    }

    let max_size_bytes = (config.max_file_upload_size_mb * 1024 * 1024) as u64;
    let mut filtered_files = Vec::new();
    for file in files {
        match tokio::fs::metadata(&file.source_path).await {
            Ok(metadata) => {
                if metadata.len() <= max_size_bytes {
                    filtered_files.push(file);
                } else {
                    warn!(
                        "File {} exceeds the maximum size limit of {} MB and will be skipped.",
                        file.source_path, config.max_file_upload_size_mb
                    );
                }
            }
            Err(_) => {
                warn!("Failed to read metadata for file: {}", file.source_path);
            }
        }
    }

    Ok(filtered_files)
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
                ).await?;
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
    mqtt_service: Arc<MqttService>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let options = &upload_request.options;

    start_logging(
        mqtt_service.clone(),
        format!("Starting upload of file {}", file_detail.source_path),
    );

    let file = File::open(&file_detail.source_path).await?;
    let use_compression = options
        .compression
        .as_ref()
        .map(|c| c.enabled)
        .unwrap_or(config.allow_compression);

    let quality = options
        .compression
        .as_ref()
        .map(|c| c.quality)
        .unwrap_or(config.compression_quality);

    let reader: Box<dyn io::AsyncRead + Unpin + Send> = if use_compression {
        Box::new(compress_stream(file, quality).await?)
    } else {
        Box::new(BufReader::new(file))
    };

    match options.upload_type.as_str() {
        "smb" => {
            smb_upload(
                reader,
                &file_detail.destination_path,
                config,
                tracker.clone(),
                mqtt_service.clone(),
            ).await?
        }
        "sftp" => {
            sftp_upload(
                reader,
                &file_detail.destination_path,
                config,
                tracker.clone(),
                mqtt_service.clone(),
            ).await?
        }
        _ => return Err("Unsupported upload type".into()),
    }

    publish_analytics(
        mqtt_service.clone(),
        "file_uploaded".to_string(),
        format!("File {} uploaded successfully.", file_detail.source_path),
    );

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
    mqtt_service: Arc<MqttService>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: io::AsyncRead + Unpin + Send,
{
    use std::process::Stdio;
    use tokio::process::Command;

    let full_destination_path = format!("{}/{}", config.smb_target_folder, destination_path);

    let smb_command = format!(
        "timeout {} smbclient //{}/{} -U {}%{} -c 'put - \"{}\"'",
        config.smb_connection_timeout_ms / 1000,
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
    mqtt_service: Arc<MqttService>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: io::AsyncRead + Unpin + Send,
{
    let tcp = tokio::time::timeout(
        Duration::from_millis(config.sftp_connection_timeout_ms),
        TcpStream::connect(format!("{}:{}", config.sftp_host, config.sftp_port)),
    )
        .await
        .map_err(|_| "SFTP connection timed out")??;

    let tcp = tcp.into_std()?;
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;
    session.userauth_password(&config.sftp_username, &config.sftp_password)?;

    let sftp = session.sftp()?;
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
