// transfer.rs
//
// Orchestrates a job: collects the files it covers, then hands each one to
// rclone (see rclone.rs) under the configured strategy, reporting status,
// progress, logs and a final result back over MQTT.

use crate::config::Config;
use crate::mqtt_service::{FileDetail, JobOptions, MqttService};
use crate::progress_tracker::{ProgressTracker, SharedState};
use crate::rclone::{self, RemoteKind};
use crate::service_utils::{publish_job_log, publish_job_result, publish_job_status};
use log::error;
use serde_json::json;
use std::error::Error;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

pub async fn run_job(
    job_id: String,
    tracker: Arc<ProgressTracker>,
    options: JobOptions,
    config: Config,
    _state: SharedState,
    mqtt_service: Arc<MqttService>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_job_log(mqtt_service.clone(), &job_id, "INFO", format!("Starting job {}", job_id));
    publish_job_status(mqtt_service.clone(), &job_id, "started", None);

    let start_time = Instant::now();

    let transfer_type = options.transfer_type.clone().unwrap_or_else(|| "smb".to_string());
    let kind = RemoteKind::from_str(&transfer_type)
        .ok_or_else(|| format!("Unsupported transfer type '{}'", transfer_type))?;

    let files_to_transfer = collect_files(&options, &config).await?;
    let total_size = estimate_total_size(&files_to_transfer).await?;
    tracker.set_total_size(total_size).await;

    publish_job_log(
        mqtt_service.clone(),
        &job_id,
        "INFO",
        format!("Collected {} files totaling {} bytes.", files_to_transfer.len(), total_size),
    );

    let rclone_config_path = rclone::write_rclone_config(&config).await?;

    let strategy = options
        .transfer_strategy
        .clone()
        .unwrap_or_else(|| config.transfer_strategy.clone());

    let mut successful = 0usize;
    let mut failed = 0usize;

    match strategy.as_str() {
        "batch" => {
            let semaphore = Arc::new(Semaphore::new(5));
            let mut tasks = Vec::new();

            for file_detail in &files_to_transfer {
                if tracker.is_cancelled() {
                    break;
                }
                let permit = semaphore.clone().acquire_owned().await?;
                let tracker = tracker.clone();
                let config = config.clone();
                let mqtt_service = mqtt_service.clone();
                let file_detail = file_detail.clone();
                let job_id = job_id.clone();
                let rclone_config_path = rclone_config_path.clone();

                tasks.push(tokio::spawn(async move {
                    let result = transfer_single_file(
                        &job_id,
                        file_detail,
                        kind,
                        &config,
                        &rclone_config_path,
                        tracker,
                        mqtt_service,
                    )
                    .await;
                    drop(permit);
                    result
                }));
            }

            for task in tasks {
                match task.await {
                    Ok(Ok(_)) => successful += 1,
                    Ok(Err(e)) => {
                        error!("Transfer error: {:?}", e);
                        failed += 1;
                    }
                    Err(_) => {
                        error!("Transfer task panicked");
                        failed += 1;
                    }
                }
            }
        }
        "sequential" => {
            for file_detail in &files_to_transfer {
                if tracker.is_cancelled() {
                    break;
                }
                match transfer_single_file(
                    &job_id,
                    file_detail.clone(),
                    kind,
                    &config,
                    &rclone_config_path,
                    tracker.clone(),
                    mqtt_service.clone(),
                )
                .await
                {
                    Ok(_) => successful += 1,
                    Err(e) => {
                        error!("Transfer error: {:?}", e);
                        failed += 1;
                    }
                }
            }
        }
        other => return Err(format!("Unsupported transfer strategy '{}'", other).into()),
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let total_files = files_to_transfer.len();
    let success_rate = if total_files > 0 {
        (successful as f64 / total_files as f64) * 100.0
    } else {
        0.0
    };
    let throughput_mb_s = if elapsed > 0.0 {
        (total_size as f64 / (1024.0 * 1024.0)) / elapsed
    } else {
        0.0
    };

    let status = if failed > 0 { "error" } else { "completed" };

    let result_json = json!({
        "jobId": job_id,
        "status": status,
        "engine": "rclone",
        "transferType": transfer_type,
        "totalFiles": total_files,
        "successfulFiles": successful,
        "failedFiles": failed,
        "successRatePercent": success_rate,
        "totalBytes": total_size,
        "elapsedSeconds": elapsed,
        "throughputMbPerSec": throughput_mb_s,
    });

    publish_job_result(mqtt_service.clone(), &job_id, result_json);
    publish_job_status(
        mqtt_service.clone(),
        &job_id,
        status,
        Some(format!("Job {} finished: {} succeeded, {} failed.", job_id, successful, failed)),
    );
    publish_job_log(mqtt_service.clone(), &job_id, "INFO", format!("Job {} finished.", job_id));

    Ok(())
}

async fn collect_files(
    options: &JobOptions,
    config: &Config,
) -> Result<Vec<FileDetail>, Box<dyn Error + Send + Sync>> {
    let mut files = Vec::new();

    let file_filters: Vec<String> = options
        .file_filters
        .clone()
        .unwrap_or_else(|| config.file_filters.clone());

    if let Some(ref recursive_folders) = options.recursive_folders {
        for folder in recursive_folders {
            let folder_files = collect_files_recursively(&folder.path, &file_filters).await?;
            files.extend(folder_files);
        }
    }

    if let Some(ref specified_files) = options.files {
        files.extend(specified_files.clone());
    }

    let max_size_bytes = (config.max_file_transfer_size_mb as u64) * 1024 * 1024;
    let mut filtered_files = Vec::new();
    for file in files {
        match tokio::fs::metadata(&file.source_path).await {
            Ok(metadata) => {
                if metadata.len() <= max_size_bytes {
                    filtered_files.push(file);
                } else {
                    log::warn!(
                        "File {} exceeds the maximum size limit of {} MB and will be skipped.",
                        file.source_path,
                        config.max_file_transfer_size_mb
                    );
                }
            }
            Err(_) => {
                log::warn!("Failed to read metadata for file: {}", file.source_path);
            }
        }
    }

    Ok(filtered_files)
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
                let sub_files = collect_files_recursively(path.to_str().unwrap_or_default(), file_filters).await?;
                files.extend(sub_files);
            } else if let Some(extension) = path.extension() {
                if file_filters.contains(&extension.to_string_lossy().to_lowercase()) {
                    files.push(FileDetail {
                        source_path: path.to_str().unwrap_or_default().to_string(),
                        destination_path: path.file_name().unwrap().to_string_lossy().to_string(),
                    });
                }
            }
        }
        Ok(files)
    })
}

async fn estimate_total_size(files: &[FileDetail]) -> Result<u64, Box<dyn Error + Send + Sync>> {
    let mut total_size = 0u64;
    for file in files {
        total_size += tokio::fs::metadata(&file.source_path).await?.len();
    }
    Ok(total_size)
}

async fn transfer_single_file(
    job_id: &str,
    file_detail: FileDetail,
    kind: RemoteKind,
    config: &Config,
    rclone_config_path: &Path,
    tracker: Arc<ProgressTracker>,
    mqtt_service: Arc<MqttService>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    publish_job_log(
        mqtt_service.clone(),
        job_id,
        "INFO",
        format!("Transferring {} -> {}", file_detail.source_path, file_detail.destination_path),
    );

    let last_reported = Arc::new(AtomicU64::new(0));
    let tracker_for_progress = tracker.clone();

    let outcome = rclone::transfer_file(
        config,
        rclone_config_path,
        kind,
        &file_detail.source_path,
        &file_detail.destination_path,
        move |bytes| {
            let previous = last_reported.swap(bytes, Ordering::SeqCst);
            let delta = bytes.saturating_sub(previous);
            if delta > 0 {
                let tracker = tracker_for_progress.clone();
                tokio::spawn(async move {
                    tracker.add_transferred(delta).await;
                });
            }
        },
    )
    .await?;

    publish_job_log(
        mqtt_service.clone(),
        job_id,
        "INFO",
        format!(
            "Transferred {} ({} bytes in {:.2}s)",
            file_detail.source_path,
            outcome.bytes_transferred,
            outcome.duration.as_secs_f64()
        ),
    );

    Ok(())
}
