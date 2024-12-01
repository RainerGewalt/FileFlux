use crate::config::Config;
use crate::mqtt_service::{FileDetail, UploadRequest};
use crate::progress_tracker::{ProgressTracker, SharedState};
use log::{error, info};
use std::error::Error;
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let files_to_upload = collect_files(&upload_request, &config).await?;
    let total_size = estimate_total_size(&files_to_upload).await?;
    tracker.set_total_size(total_size);

    // Verwendung einer Semaphore für kontrollierte Parallelität
    let semaphore = Arc::new(Semaphore::new(5)); // Anzahl der parallelen Uploads
    let mut tasks = Vec::new();

    for file_detail in files_to_upload {
        let permit = semaphore.clone().acquire_owned().await?;
        let tracker = tracker.clone();
        let config = config.clone();
        let upload_request = upload_request.clone();

        tasks.push(tokio::spawn(async move {
            let result = upload_single_file(file_detail, &upload_request, &config, tracker).await;
            drop(permit); // Freigabe der Semaphore
            result
        }));
    }

    // Warten auf alle Uploads
    for task in tasks {
        if let Err(e) = task.await.unwrap_or_else(|_| Err("Task panicked".into())) {
            error!("Fehler beim Upload: {:?}", e);
        }
    }

    info!("Upload erfolgreich abgeschlossen.");
    Ok(())
}

async fn collect_files(
    upload_request: &UploadRequest,
    config: &Config,
) -> Result<Vec<FileDetail>, Box<dyn Error + Send + Sync>> {
    if upload_request.recursive.unwrap_or(false) {
        let root_folder = upload_request
            .root_folder
            .as_deref()
            .unwrap_or(&config.default_file_source);
        let file_filters = upload_request
            .file_filters
            .as_ref()
            .unwrap_or(&config.file_filters);
        collect_files_recursively(root_folder, file_filters).await
    } else if let Some(ref files) = upload_request.files {
        Ok(files.clone())
    } else {
        Err("Keine Dateien zum Upload angegeben.".into())
    }
}

fn collect_files_recursively<'a>(
    root_folder: &'a str,
    file_filters: &'a [String],
) -> Pin<Box<dyn Future<Output = Result<Vec<FileDetail>, Box<dyn Error + Send + Sync>>> + Send + 'a>> {
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
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let file = File::open(&file_detail.source_path).await?;
    let reader: Box<dyn io::AsyncRead + Unpin + Send> =
        if upload_request
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
            smb_upload(reader, &file_detail.destination_path, config, tracker).await?
        }
        "sftp" => {
            sftp_upload(reader, &file_detail.destination_path, config, tracker).await?
        }
        _ => return Err("Nicht unterstützter Upload-Typ".into()),
    }

    Ok(())
}

async fn smb_upload<R>(
    mut reader: R,
    destination_path: &str,
    config: &Config,
    tracker: Arc<ProgressTracker>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: io::AsyncRead + Unpin + Send,
{
    use std::process::Stdio;
    use tokio::process::Command;

    let smb_command = format!(
        "smbclient //{}/{} -U {}%{} -c 'put - \"{}\"'",
        config.smb_target_ip,
        config.smb_share_name,
        config.smb_username,
        config.smb_password,
        destination_path
    );

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&smb_command)
        .stdin(Stdio::piped())
        .spawn()?;

    let mut stdin = BufWriter::new(child.stdin.take().ok_or("Fehler beim Öffnen von stdin")?);
    let mut buffer = [0; CHUNK_SIZE];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        stdin.write_all(&buffer[..n]).await?;
        tracker.update_progress(n as u64).await;
    }
    stdin.flush().await?;
    let status = child.wait().await?;
    if !status.success() {
        return Err("smbclient-Befehl fehlgeschlagen".into());
    }

    Ok(())
}

async fn sftp_upload<R>(
    mut reader: R,
    destination_path: &str,
    config: &Config,
    tracker: Arc<ProgressTracker>,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    R: io::AsyncRead + Unpin + Send,
{
    let tcp = TcpStream::connect(format!("{}:{}", config.sftp_host, config.sftp_port)).await?;
    let tcp = tcp.into_std()?;
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;
    session.userauth_password(&config.sftp_username, &config.sftp_password)?;

    let sftp = session.sftp()?;
    let mut remote_file = sftp.create(Path::new(destination_path))?;
    let mut buffer = [0; CHUNK_SIZE];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        remote_file.write_all(&buffer[..n])?;
        tracker.update_progress(n as u64).await;
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

