use crate::progress_tracker::{ProgressTracker, SharedState};
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use serde::Deserialize;
use serde_json;
use uuid::Uuid;

/// A command received on `trailtransfer/{worker_id}/commands`.
#[derive(Deserialize, Debug, Clone)]
pub struct JobRequest {
    pub action: String, // "start" or "stop"
    pub options: Option<JobOptions>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct JobOptions {
    pub transfer_type: Option<String>, // "smb" or "sftp"
    pub job_id: Option<String>,        // job to stop, for action "stop"
    pub recursive_folders: Option<Vec<FolderConfig>>,
    pub files: Option<Vec<FileDetail>>,
    pub file_filters: Option<Vec<String>>,
    pub transfer_strategy: Option<String>, // "batch" or "sequential"
}

#[derive(Deserialize, Debug, Clone)]
pub struct FolderConfig {
    pub path: String,
    pub recursive: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FileDetail {
    pub source_path: String,
    pub destination_path: String,
}

#[derive(Debug)]
enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

use crate::config::Config;
use crate::transfer;

pub struct MqttService {
    client_state: Mutex<ClientState>,
    client: Mutex<Option<AsyncClient>>,
    state: SharedState,
    pub(crate) config: Config,
}

impl MqttService {
    pub fn new(state: SharedState, config: Config) -> Arc<Self> {
        Arc::new(Self {
            client_state: Mutex::new(ClientState::Disconnected),
            client: Mutex::new(None),
            state,
            config,
        })
    }

    pub async fn start(self: Arc<Self>, mqtt_host: &str, mqtt_port: u16, mqtt_client_id: &str) {
        info!("Starting MQTT service...");

        let initial_retry_interval = Duration::from_millis(self.config.mqtt_retry_interval_ms);
        let max_retries = std::cmp::min(
            if self.config.mqtt_max_retries > 0 {
                self.config.mqtt_max_retries
            } else {
                5
            },
            100,
        );
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
                mqtt_options.set_credentials(&self.config.mqtt_username, &self.config.mqtt_password);
            }

            // Last will: if this worker drops off without a clean shutdown,
            // the broker announces it on the same health topic used for
            // heartbeats, so consumers can't mistake a dead worker for a
            // quiet one.
            mqtt_options.set_last_will(rumqttc::LastWill::new(
                self.config.health_topic(),
                r#"{"status": "offline"}"#,
                QoS::AtLeastOnce,
                true,
            ));

            let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

            {
                let mut client_lock = self.client.lock().await;
                *client_lock = Some(client.clone());
            }

            {
                let mut client_state = self.client_state.lock().await;
                *client_state = ClientState::Connecting;
            }

            let control_topic = self.config.command_topic();
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

            warn!("Lost connection to MQTT broker. Retrying in {:?}...", retry_interval);
            retries += 1;
            sleep(retry_interval).await;
            retry_interval = (retry_interval * 2).min(Duration::from_secs(60));
        }
    }

    async fn handle_event(self: Arc<Self>, event: Event) {
        match event {
            Event::Incoming(Packet::Publish(publish)) => {
                let topic = publish.topic.clone();
                let payload = String::from_utf8(publish.payload.to_vec()).unwrap_or_else(|_| "".to_string());

                let control_topic = self.config.command_topic();
                if topic == control_topic {
                    self.handle_command(payload).await;
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

    pub async fn publish_message(&self, topic: &str, message: &str, qos: QoS, retain: bool) {
        for _ in 0..5 {
            let client = self.client.lock().await;
            if let Some(client) = client.as_ref() {
                match client.publish(topic, qos, retain, message).await {
                    Ok(_) => {
                        info!("Message published to '{}': {}", topic, message);
                        return;
                    }
                    Err(e) => {
                        error!("Failed to publish message to '{}': {:?}", topic, e);
                    }
                }
            } else {
                error!("MQTT client is not connected. Retrying...");
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        error!("Failed to publish message to topic '{}' after multiple retries: {}", topic, message);
    }

    async fn handle_command(self: Arc<Self>, payload: String) {
        let job_request: JobRequest = match serde_json::from_str(&payload) {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to parse command JSON: {:?}", e);
                return;
            }
        };

        info!("Received command: {:?}", job_request);

        match job_request.action.as_str() {
            "start" => {
                if let Some(options) = job_request.options {
                    let job_id = Uuid::new_v4().to_string();
                    let tracker = Arc::new(ProgressTracker::new(0, self.clone(), job_id.clone()));

                    self.state.lock().await.insert(job_id.clone(), tracker.clone());

                    let state_clone = self.state.clone();
                    let config_clone = self.config.clone();
                    let mqtt_service = self.clone();

                    tokio::spawn(async move {
                        if let Err(e) = transfer::run_job(
                            job_id,
                            tracker,
                            options,
                            config_clone,
                            state_clone,
                            mqtt_service,
                        )
                        .await
                        {
                            error!("Job failed: {:?}", e);
                        }
                    });
                } else {
                    error!("JobRequest options are missing for 'start' action.");
                }
            }
            "stop" => {
                let mut state = self.state.lock().await;
                let job_id = job_request.options.as_ref().and_then(|o| o.job_id.clone());

                match job_id {
                    Some(job_id) => {
                        if let Some(tracker) = state.remove(&job_id) {
                            tracker.stop().await;
                            info!("Successfully stopped job: {}", job_id);
                        } else {
                            warn!("Stop requested for unknown job: {}", job_id);
                        }
                    }
                    None => {
                        let keys: Vec<String> = state.keys().cloned().collect();
                        for key in keys {
                            if let Some(tracker) = state.remove(&key) {
                                tracker.stop().await;
                                info!("Successfully stopped job: {}", key);
                            }
                        }
                    }
                }
            }
            _ => {
                warn!("Unknown action received: {}", job_request.action);
            }
        }
    }
}
