use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

mod config;
mod mqtt_service;
mod progress_tracker;
mod upload;

use config::Config;
use mqtt_service::MqttService;
use progress_tracker::SharedState;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load configuration
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Error loading configuration: {:?}", e);
            return;
        }
    };

    // Shared state for progress tracking
    let state: SharedState = Arc::new(Mutex::new(HashMap::new()));

    // Start MQTT service
    let mqtt_service = MqttService::new(state.clone());

    let mqtt_service_clone = mqtt_service.clone();
    let mqtt_host = config.mqtt_host.clone();
    let mqtt_port = config.mqtt_port;
    let uuid = Uuid::new_v4();
    let mqtt_client_id = format!("sftp_worker{}", uuid);
    tokio::spawn(async move {
        mqtt_service_clone
            .start(&mqtt_host, mqtt_port, &mqtt_client_id)
            .await;
    });

    // Wait for termination signal
    if let Err(e) = tokio::signal::ctrl_c().await {
        error!("Failed to handle termination signal: {:?}", e);
    }
    info!("Service is shutting down...");
}