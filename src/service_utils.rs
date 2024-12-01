// service_utils.rs

use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::mqtt_service::MqttService;

/// Start the MQTT service
pub fn start_mqtt_service(mqtt_service: Arc<MqttService>) {
    // Access config via mqtt_service
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
                "logs",
                &format!("{{\"level\": \"INFO\", \"message\": \"{}\"}}", message),
                rumqttc::QoS::AtLeastOnce,
                false,
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
                "analytics",
                &format!("{{\"event\": \"{}\", \"details\": \"{}\"}}", event, details),
                rumqttc::QoS::AtLeastOnce,
                false,
            )
            .await;
    });
}

/// Handle graceful shutdown and publish shutdown progress
pub async fn handle_shutdown(mqtt_service: Arc<MqttService>) {
    if let Err(e) = tokio::signal::ctrl_c().await {
        error!("Failed to handle termination signal: {:?}", e);

        mqtt_service
            .publish_message(
                "logs",
                &format!(
                    "{{\"level\": \"ERROR\", \"message\": \"Termination signal failed: {:?}\"}}",
                    e
                ),
                rumqttc::QoS::AtLeastOnce,
                false,
            )
            .await;
    } else {
        mqtt_service
            .publish_message(
                "progress",
                "{\"status\": \"shutdown\", \"message\": \"Service is shutting down...\"}",
                rumqttc::QoS::AtLeastOnce,
                false,
            )
            .await;

        info!("Service is shutting down...");
    }
}
