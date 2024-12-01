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

        let mut retry_interval = Duration::from_secs(5); // Initial retry interval

        loop {
            debug!("Configuring MQTT broker at {}:{}", mqtt_host, mqtt_port);

            let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
            mqtt_options.set_keep_alive(Duration::from_secs(10)); // Heartbeat every 10 seconds
            mqtt_options.set_clean_session(true); // Clear old sessions
            mqtt_options.set_credentials(&self.config.mqtt_username, &self.config.mqtt_username); // Set credentials

            let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

            {
                let mut client_lock = self.client.lock().await;
                *client_lock = Some(client.clone());
            }

            {
                let mut client_state = self.client_state.lock().await;
                *client_state = ClientState::Connecting;
            }

            // Attempt to subscribe to the root topic
            let control_topic = format!("{}/control", self.config.mqtt_root_topic);
            match client.subscribe(&control_topic, QoS::AtLeastOnce).await {
                Ok(_) => {
                    info!("Successfully subscribed to topic '{}'.", control_topic);
                    {
                        let mut client_state = self.client_state.lock().await;
                        *client_state = ClientState::Connected;
                    }
                    retry_interval = Duration::from_secs(5); // Reset retry interval
                }
                Err(e) => {
                    error!("Subscription to '{}' failed: {}", control_topic, e);
                    {
                        let mut client_state = self.client_state.lock().await;
                        *client_state = ClientState::Error(e.to_string());
                    }
                    sleep(retry_interval).await;
                    retry_interval = (retry_interval * 2).min(Duration::from_secs(60)); // Exponential backoff
                    continue; // Retry connection
                }
            }

            // Event loop
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
                        break; // Exit inner loop to attempt reconnection
                    }
                }
            }

            warn!("Lost connection to MQTT broker. Retrying in {:?}...", retry_interval);
            sleep(retry_interval).await;
            retry_interval = (retry_interval * 2).min(Duration::from_secs(60)); // Exponential backoff
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
