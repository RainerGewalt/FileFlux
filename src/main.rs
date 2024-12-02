// main.rs

mod config;
mod mqtt_service;
mod progress_tracker;
mod upload;
mod service_utils;
mod scan_directory_structure;

use crate::config::Config;
use crate::mqtt_service::MqttService;
use crate::progress_tracker::SharedState;
use crate::service_utils::{handle_shutdown, periodic_status_update, publish_directory_structure, publish_status, start_logging, start_mqtt_service};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error};
#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load configuration
    let config = match Config::from_env() {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => {
            error!("Error loading configuration: {:?}", e);
            return;
        }
    };

    // Shared state for progress tracking
    let state: SharedState = Arc::new(Mutex::new(HashMap::new()));

    // Create MQTT service
    let mqtt_service = MqttService::new(state.clone(), (*config).clone()); // Clone Config for MqttService

    // Start MQTT service
    start_mqtt_service(mqtt_service.clone());

    // Start logging
    start_logging(mqtt_service.clone(), "Service is starting...".to_string());

    // Start periodic status updates
    periodic_status_update(mqtt_service.clone());

    // Log start status
    publish_status(
        mqtt_service.clone(),
        "running".to_string(),
        Some("Uploader service has started successfully.".to_string()),
    );
    // Start publishing directory structure
    let output_folder_path = config.output_folder.clone();
    let directory_topic = config.directory_topic.clone();
    publish_directory_structure(mqtt_service.clone(), output_folder_path, directory_topic);

    // Handle shutdown
    handle_shutdown(mqtt_service.clone()).await;

    // Log shutdown status
    publish_status(
        mqtt_service.clone(),
        "shutdown".to_string(),
        Some("Uploader service is shutting down.".to_string()),
    );
}

