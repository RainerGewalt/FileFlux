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

#[tokio::main]
async fn main() {
    // Logging initialisieren
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    // Konfiguration laden
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Fehler beim Laden der Konfiguration: {:?}", e);
            return;
        }
    };

    // Gemeinsamer Zustand für Fortschrittstracking
    let state: SharedState = Arc::new(Mutex::new(HashMap::new()));

    // MQTT-Service starten
    let mut mqtt_service = MqttService::new(state.clone());
    tokio::spawn(async move {
        mqtt_service
            .start(&config.mqtt_address, config.mqtt_port, &config.instance_id)
            .await;
    });

    // Warten auf Beendigungssignal
    tokio::signal::ctrl_c().await.unwrap();
    info!("Beende den Service...");
}
