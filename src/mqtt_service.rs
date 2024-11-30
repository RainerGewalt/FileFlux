use crate::progress_tracker::{ProgressTracker, SharedState};
use crate::upload::process_and_upload;
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::lookup_host;
use tokio::time::{sleep, Duration};

#[derive(Debug)]
enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
pub struct MqttService {
    client_state: Mutex<ClientState>,
    client: Mutex<Option<AsyncClient>>,
    state: SharedState,
    root_topic: String,  // Root topic for MQTT messages
    username: String,    // MQTT username
    password: String,    // MQTT password
}

impl MqttService {
    pub fn new(state: SharedState, root_topic: String, username: String, password: String) -> Arc<Self> {
        Arc::new(Self {
            client_state: Mutex::new(ClientState::Disconnected),
            client: Mutex::new(None),
            state,
            root_topic,
            username,
            password,
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
            mqtt_options.set_credentials(&self.username, &self.password); // Set credentials

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
            let control_topic = format!("{}/control", self.root_topic);
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

                let control_topic = format!("{}/control", self.root_topic);
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
        let start_prefix = format!("{}/start ", self.root_topic);
        let cancel_prefix = format!("{}/cancel ", self.root_topic);

        if let Some(cmd) = payload.strip_prefix(&start_prefix) {
            let file = cmd.trim().to_string();
            info!("Starting upload for file: {}", file);

            let client = self.client.lock().await.clone().unwrap();

            let tracker = Arc::new(ProgressTracker::new(
                1_000_000, // Example total size
                client,
                file.clone(),
            ));
            self.state.lock().await.insert(file.clone(), tracker.clone());

            let state_clone = self.state.clone();
            tokio::spawn(async move {
                if let Err(e) = process_and_upload(tracker, state_clone).await {
                    error!("Upload failed for file {}: {:?}", file, e);
                }
            });
        } else if let Some(cmd) = payload.strip_prefix(&cancel_prefix) {
            let file = cmd.trim().to_string();
            self.state.lock().await.remove(&file);
            info!("Upload canceled for file: {}", file);
        } else {
            warn!("Unknown command received: {}", payload);
        }
    }
}
