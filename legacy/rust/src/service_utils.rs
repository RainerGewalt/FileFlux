use crate::mqtt_service::MqttService;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tracing::error;

/// Start the MQTT service
pub fn start_mqtt_service(mqtt_service: Arc<MqttService>) {
    let mqtt_host = mqtt_service.config.mqtt_host.clone();
    let mqtt_port = mqtt_service.config.mqtt_port;
    let client_id = format!("trailtransfer-{}", mqtt_service.config.worker_id);

    let mqtt_service_clone = mqtt_service.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .start(&mqtt_host, mqtt_port, &client_id)
            .await;
    });
}

/// Publishes this worker's static capabilities, retained, once at startup.
/// Lets a controller (or TrailMQ) discover what a worker can do without
/// hardcoding assumptions about its backends or limits.
pub fn publish_capabilities(mqtt_service: Arc<MqttService>) {
    let config = mqtt_service.config.clone();
    let topic = config.capabilities_topic();
    let payload = json!({
        "workerId": config.worker_id,
        "version": env!("CARGO_PKG_VERSION"),
        "engine": "rclone",
        "backends": ["smb", "sftp"],
        "transferStrategies": ["batch", "sequential"],
        "maxFileTransferSizeMb": config.max_file_transfer_size_mb,
    })
    .to_string();

    tokio::spawn(async move {
        mqtt_service
            .publish_message(&topic, &payload, rumqttc::QoS::AtLeastOnce, true)
            .await;
    });
}

/// Publishes a retained heartbeat to the worker's health topic every 30s.
pub fn start_health_loop(mqtt_service: Arc<MqttService>) {
    let topic = mqtt_service.config.health_topic();
    let started_at = Instant::now();

    tokio::spawn(async move {
        loop {
            let payload = json!({
                "status": "healthy",
                "uptimeSeconds": started_at.elapsed().as_secs(),
            })
            .to_string();

            mqtt_service
                .publish_message(&topic, &payload, rumqttc::QoS::AtLeastOnce, true)
                .await;

            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    });
}

/// Publishes a job lifecycle status update.
pub fn publish_job_status(mqtt_service: Arc<MqttService>, job_id: &str, status: &str, details: Option<String>) {
    let topic = mqtt_service.config.job_status_topic(job_id);
    let payload = json!({
        "jobId": job_id,
        "status": status,
        "details": details.unwrap_or_default(),
    })
    .to_string();

    tokio::spawn(async move {
        mqtt_service
            .publish_message(&topic, &payload, rumqttc::QoS::AtLeastOnce, true)
            .await;
    });
}

/// Publishes byte-level job progress.
pub fn publish_job_progress(mqtt_service: Arc<MqttService>, job_id: &str, transferred: u64, total: u64) {
    let topic = mqtt_service.config.job_progress_topic(job_id);
    let percentage = if total > 0 {
        (transferred as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let payload = json!({
        "jobId": job_id,
        "transferred": transferred,
        "total": total,
        "percentage": percentage,
    })
    .to_string();

    tokio::spawn(async move {
        mqtt_service
            .publish_message(&topic, &payload, rumqttc::QoS::AtLeastOnce, true)
            .await;
    });
}

/// Publishes the final, structured evidence record for a completed job.
pub fn publish_job_result(mqtt_service: Arc<MqttService>, job_id: &str, result: serde_json::Value) {
    let topic = mqtt_service.config.job_result_topic(job_id);
    let payload = result.to_string();

    tokio::spawn(async move {
        mqtt_service
            .publish_message(&topic, &payload, rumqttc::QoS::AtLeastOnce, true)
            .await;
    });
}

/// Publishes a single job-scoped log line.
pub fn publish_job_log(mqtt_service: Arc<MqttService>, job_id: &str, level: &str, message: String) {
    let topic = mqtt_service.config.job_logs_topic(job_id);
    let payload = json!({
        "jobId": job_id,
        "level": level,
        "message": message,
    })
    .to_string();

    tokio::spawn(async move {
        mqtt_service
            .publish_message(&topic, &payload, rumqttc::QoS::AtLeastOnce, false)
            .await;
    });
}

/// Handles graceful shutdown, publishing a final health event before exit.
pub async fn handle_shutdown(mqtt_service: Arc<MqttService>) {
    let topic = mqtt_service.config.health_topic();

    if let Err(e) = tokio::signal::ctrl_c().await {
        error!("Failed to handle termination signal: {:?}", e);
        mqtt_service
            .publish_message(
                &topic,
                r#"{"status": "error", "details": "termination signal handling failed"}"#,
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
        return;
    }

    mqtt_service
        .publish_message(
            &topic,
            r#"{"status": "stopped"}"#,
            rumqttc::QoS::AtLeastOnce,
            true,
        )
        .await;
}
