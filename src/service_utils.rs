

use uuid::Uuid;
use crate::scan_directory_structure::scan_directory_structure;
use std::sync::Arc;
use tokio::time::{ Duration};
use tracing::{error, info};
use crate::mqtt_service::MqttService;

/// Start the MQTT service
pub fn start_mqtt_service(mqtt_service: Arc<MqttService>) {
    let mqtt_host = mqtt_service.config.mqtt_host.clone();
    let mqtt_port = mqtt_service.config.mqtt_port;
    let mqtt_client_id = format!("mqtt_service_{}", Uuid::new_v4());

    let mqtt_service_clone = mqtt_service.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .start(&mqtt_host, mqtt_port, &mqtt_client_id)
            .await;
    });
}

/// Start logging with the MQTT service
pub fn start_logging(mqtt_service: Arc<MqttService>, message: String) {
    let mqtt_service_clone = mqtt_service.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &mqtt_service_clone.config.log_topic,
                &format!("{{\"level\": \"INFO\", \"message\": \"{}\"}}", message),
                rumqttc::QoS::AtLeastOnce,
                    true,
            )
            .await;
    });
}

/// Publish analytics events with the MQTT service
pub fn publish_analytics(mqtt_service: Arc<MqttService>, event: String, details: String) {
    let mqtt_service_clone = mqtt_service.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &mqtt_service_clone.config.analytics_topic,
                &format!("{{\"event\": \"{}\", \"details\": \"{}\"}}", event, details),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    });
}

/// Publish progress updates with the MQTT service
pub fn publish_progress(mqtt_service: Arc<MqttService>, progress: u64, total: u64) {
    let mqtt_service_clone = mqtt_service.clone();
    let topic = mqtt_service_clone.config.progress_topic.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &topic,
                &format!(
                    "{{\"progress\": {}, \"total\": {}, \"percentage\": {:.2}}}",
                    progress,
                    total,
                    (progress as f64 / total as f64) * 100.0
                ),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    });
}

pub fn publish_directory_structure(
    mqtt_service: Arc<MqttService>,
    folder_path: String,
    topic: String,
) {
    tokio::spawn(async move {
        loop {
            // Verzeichnis scannen
            let directory_structure = scan_directory_structure(&folder_path);

            // JSON-Payload erstellen
            let payload = match directory_structure {
                serde_json::Value::Array(_) => {
                    // Erfolgreich gescannt
                    serde_json::json!({
                        "folder": folder_path,
                        "structure": directory_structure
                    })
                        .to_string()
                }
                _ => {
                    // Fehler beim Scannen
                    error!(
                        "Failed to scan directory structure at path: {}",
                        folder_path
                    );
                    serde_json::json!({
                        "folder": folder_path,
                        "error": "Failed to scan directory"
                    })
                        .to_string()
                }
            };

            // Nachricht veröffentlichen
            mqtt_service.publish_message(&topic, &payload, rumqttc::QoS::AtLeastOnce, true).await;

            // Informationen protokollieren
            info!("Directory structure published to topic '{}'.", topic);

            // Pause zwischen Iterationen
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}


/// Publish uploader status with the MQTT service
pub fn publish_status(mqtt_service: Arc<MqttService>, status: String, details: Option<String>) {
    let mqtt_service_clone = mqtt_service.clone();
    let topic = mqtt_service_clone.config.status_topic.clone();
    let details_message = details.unwrap_or_default();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &topic,
                &format!(
                    "{{\"status\": \"{}\", \"details\": \"{}\"}}",
                    status, details_message
                ),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    });
}

/// Handle graceful shutdown and publish shutdown progress
pub async fn handle_shutdown(mqtt_service: Arc<MqttService>) {
    let status_topic = mqtt_service.config.status_topic.clone();

    if let Err(e) = tokio::signal::ctrl_c().await {
        error!("Failed to handle termination signal: {:?}", e);

        mqtt_service
            .publish_message(
                &status_topic,
                "{\"status\": \"error\", \"message\": \"Termination signal failed\"}",
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    } else {
        mqtt_service
            .publish_message(
                &status_topic,
                "{\"status\": \"shutdown\", \"message\": \"Uploader is shutting down...\"}",
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;

        info!("Uploader is shutting down...");
    }
}

/// Start periodic status updates
pub fn periodic_status_update(mqtt_service: Arc<MqttService>) {
    let topic = mqtt_service.config.status_topic.clone();
    tokio::spawn(async move {
        loop {
            mqtt_service
                .publish_message(
                    &topic,
                    "{\"status\": \"running\", \"message\": \"Uploader is operational\"}",
                    rumqttc::QoS::AtLeastOnce,
                    true,
                )
                .await;
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    });
}
