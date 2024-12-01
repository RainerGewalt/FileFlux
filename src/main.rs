// main.rs

mod config;
mod mqtt_service;
mod progress_tracker;
mod upload;
mod service_utils;

use crate::config::Config;
use crate::mqtt_service::MqttService;
use crate::progress_tracker::SharedState;
use crate::service_utils::{handle_shutdown, publish_analytics, start_logging, start_mqtt_service};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
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

    // Publish analytics
    publish_analytics(
        mqtt_service.clone(),
        "mqtt_connected".to_string(),
        "Service connected to MQTT broker".to_string(),
    );

    // Handle shutdown
    handle_shutdown(mqtt_service.clone()).await;

    info!("Service has shut down.");
}
