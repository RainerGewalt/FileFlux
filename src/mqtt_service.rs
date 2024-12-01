use crate::progress_tracker::{ProgressTracker, SharedState};
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use serde::Deserialize;
use serde_json;
use uuid::Uuid;

#[derive(Debug, Deserialize, Clone)]
pub struct UploadRequest {
    #[serde(rename = "type")]
    pub upload_type: String, // "smb" or "sftp"
    pub recursive: Option<bool>,
    pub root_folder: Option<String>,
    pub files: Option<Vec<FileDetail>>,
    pub compression: Option<CompressionConfig>,
    pub file_filters: Option<Vec<String>>,
    pub upload_strategy: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FileDetail {
    pub source_path: String,
    pub destination_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CompressionConfig {
    pub enabled: bool,
    pub quality: u8,
}


#[derive(Debug)]
enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
use crate::config::Config;
use crate::upload;

pub struct MqttService {
    client_state: Mutex<ClientState>,
    client: Mutex<Option<AsyncClient>>,
    state: SharedState,  // Root topic for MQTT messages
    config: Config,
}

impl MqttService {
    pub fn new(state: SharedState, config: Config) -> Arc<Self> {
        Arc::new(Self {
            client_state: Mutex::new(ClientState::Disconnected),
            client: Mutex::new(None),
            state,
            config
        })
    }
    pub async fn start(self: Arc<Self>, mqtt_host: &str, mqtt_port: u16, mqtt_client_id: &str) {
        info!("Starting MQTT service...");

        let initial_retry_interval = Duration::from_millis(self.config.mqtt_retry_interval_ms);
        let max_retries = std::cmp::min(if self.config.mqtt_max_retries > 0 {
            self.config.mqtt_max_retries
        } else {
            5
        }, 100); // Default to 5, cap at 100
        let mut retry_interval = initial_retry_interval;
        let mut retries = 0;

        loop {
            if max_retries != -1 && retries >= max_retries {
                error!("Maximum number of retries ({}) reached. Stopping the service.", max_retries);
                break;
            }

            debug!("Configuring MQTT broker at {}:{}...", mqtt_host, mqtt_port);

            let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
            mqtt_options.set_keep_alive(Duration::from_secs(10));
            mqtt_options.set_clean_session(true);

            if !self.config.mqtt_username.is_empty() && !self.config.mqtt_password.is_empty() {
                mqtt_options.set_credentials(
                    &self.config.mqtt_username,
                    &self.config.mqtt_password,
                );
            }

            let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

            {
                let mut client_lock = self.client.lock().await;
                *client_lock = Some(client.clone());
            }

            {
                let mut client_state = self.client_state.lock().await;
                *client_state = ClientState::Connecting;
            }

            let control_topic = format!("{}/control", self.config.mqtt_root_topic);
            match client.subscribe(&control_topic, QoS::AtLeastOnce).await {
                Ok(_) => {
                    info!("Successfully subscribed to topic '{}'.", control_topic);
                    {
                        let mut client_state = self.client_state.lock().await;
                        *client_state = ClientState::Connected;
                    }
                    retry_interval = initial_retry_interval;
                }
                Err(e) => {
                    error!("Failed to subscribe to topic '{}': {}", control_topic, e);
                    {
                        let mut client_state = self.client_state.lock().await;
                        *client_state = ClientState::Error(e.to_string());
                    }
                    retries += 1;
                    sleep(retry_interval).await;
                    retry_interval = (retry_interval * 2).min(Duration::from_secs(60));
                    continue;
                }
            }

            loop {
                match eventloop.poll().await {
                    Ok(event) => {
                        let self_clone = self.clone();
                        tokio::spawn(async move {
                            self_clone.handle_event(event).await;
                        });
                    }
                    Err(e) => {
                        error!("Error in MQTT event loop: {:?}", e);
                        {
                            let mut client_state = self.client_state.lock().await;
                            *client_state = ClientState::Disconnected;
                        }
                        break;
                    }
                }
            }

            warn!(
            "Lost connection to MQTT broker. Retrying in {:?}...",
            retry_interval
        );
            retries += 1;
            sleep(retry_interval).await;
            retry_interval = (retry_interval * 2).min(Duration::from_secs(60));
        }
    }







    async fn handle_event(self: Arc<Self>, event: Event) {
        match event {
            Event::Incoming(Packet::Publish(publish)) => {
                let topic = publish.topic.clone();
                let payload =
                    String::from_utf8(publish.payload.to_vec()).unwrap_or_else(|_| "".to_string());

                let control_topic = format!("{}/control", self.config.mqtt_root_topic);
                if topic == control_topic {
                    self.handle_upload_command(payload).await;
                } else {
                    warn!("Unknown topic received: {}", topic);
                }
            }
            Event::Incoming(Packet::ConnAck(_)) => {
                info!("Connected to MQTT broker.");
            }
            Event::Outgoing(_) => {
                debug!("Outgoing event.");
            }
            _ => {
                debug!("Unhandled event: {:?}", event);
            }
        }
    }
    pub async fn publish_message(
        &self,
        topic_suffix: &str,
        message: &str,
        qos: QoS,
        retain: bool,
    ) {
        let client = self.client.lock().await;
        if let Some(client) = client.as_ref() {
            let full_topic = format!(
                "{}/{}",
                self.config.mqtt_root_topic.trim_end_matches('/'),
                topic_suffix.trim_start_matches('/')
            );

            match client.publish(full_topic.clone(), qos, retain, message).await {
                Ok(_) => {
                    info!("Message published to '{}': {}", full_topic, message);
                }
                Err(e) => {
                    error!("Failed to publish message to '{}': {:?}", full_topic, e);
                }
            }
        } else {
            error!("MQTT client is not connected. Unable to publish message.");
        }
    }

    async fn handle_upload_command(&self, payload: String) {
        // Parse the JSON payload
        let upload_request: UploadRequest = match serde_json::from_str(&payload) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse upload command JSON: {:?}", e);
                return;
            }
        };

        info!("Received upload request: {:?}", upload_request);

        let client = self.client.lock().await.clone().unwrap();

        // Generate a unique identifier for the upload task
        let upload_task_id = Uuid::new_v4().to_string();

        let tracker = Arc::new(ProgressTracker::new(
            1_000_000u64,
            client,
            upload_task_id.clone(),
        ));


        // Store the tracker in shared state
        self.state.lock().await.insert(upload_task_id.clone(), tracker.clone());

        let state_clone = self.state.clone();
        let upload_request_clone = upload_request.clone();
        let config_clone = self.config.clone();

        tokio::spawn(async move {
            if let Err(e) = upload::process_and_upload(
                tracker,
                upload_request_clone,
                config_clone,
                state_clone,
            )
                .await
            {
                error!("Upload failed: {:?}", e);
            }
        });
    }
}
