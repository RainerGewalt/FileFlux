// main.rs

mod config;
mod mqtt_service;
mod progress_tracker;
mod rclone;
mod service_utils;
mod transfer;

use crate::config::Config;
use crate::mqtt_service::MqttService;
use crate::progress_tracker::SharedState;
use crate::service_utils::{
    handle_shutdown, publish_capabilities, start_health_loop, start_mqtt_service,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = match Config::from_env() {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => {
            error!("Error loading configuration: {:?}", e);
            return;
        }
    };

    info!("Starting TrailTransfer worker '{}'...", config.worker_id);

    let state: SharedState = Arc::new(Mutex::new(HashMap::new()));

    let mqtt_service = MqttService::new(state.clone(), (*config).clone());

    start_mqtt_service(mqtt_service.clone());
    publish_capabilities(mqtt_service.clone());
    start_health_loop(mqtt_service.clone());

    handle_shutdown(mqtt_service.clone()).await;

    info!("TrailTransfer worker '{}' shutting down.", config.worker_id);
}
