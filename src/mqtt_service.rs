use crate::progress_tracker::{ProgressTracker, SharedState};
use crate::upload::process_and_upload;
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use tokio::sync::Mutex;
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
}

impl MqttService {
    pub fn new(state: SharedState) -> Arc<Self> {
        Arc::new(Self {
            client_state: Mutex::new(ClientState::Disconnected),
            client: Mutex::new(None),
            state,
        })
    }

    pub async fn start(self: Arc<Self>, mqtt_host: &str, mqtt_port: u16, mqtt_client_id: &str) {
        info!("Starting MQTT service...");
        loop {
            debug!("Configuring broker at {}:{}", mqtt_host, mqtt_port);

            let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
            mqtt_options.set_keep_alive(Duration::from_secs(10)); // Heartbeat every 10 seconds
            mqtt_options.set_clean_session(true); // Clear old sessions

            let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

            {
                let mut client_lock = self.client.lock().await;
                *client_lock = Some(client.clone());
            }

            {
                let mut client_state = self.client_state.lock().await;
                *client_state = ClientState::Connecting;
            }

            // Attempt to subscribe to the topic
            match client.subscribe("upload/control", QoS::AtLeastOnce).await {
                Ok(_) => {
                    info!("Successfully subscribed to topic 'upload/control'.");
                    let mut client_state = self.client_state.lock().await;
                    *client_state = ClientState::Connected;
                }
                Err(e) => {
                    error!("Subscription failed: {}", e);
                    let mut client_state = self.client_state.lock().await;
                    *client_state = ClientState::Error(e.to_string());
                    sleep(Duration::from_secs(5)).await;
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
                        sleep(Duration::from_secs(5)).await;
                        break; // Exit inner loop to attempt reconnection
                    }
                }
            }

            // At this point, the inner loop has exited due to an error
            // The outer loop will attempt to reconnect
            warn!("Reconnecting to MQTT broker...");
        }
    }

    async fn handle_event(self: Arc<Self>, event: Event) {
        match event {
            Event::Incoming(Packet::Publish(publish)) => {
                let topic = publish.topic.clone();
                let payload =
                    String::from_utf8(publish.payload.to_vec()).unwrap_or_else(|_| "".to_string());

                match topic.as_str() {
                    "upload/control" => {
                        self.handle_upload_command(payload).await;
                    }
                    _ => {
                        warn!("Unknown topic received: {}", topic);
                    }
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
        if let Some(cmd) = payload.strip_prefix("start ") {
            let file = cmd.trim().to_string();
            info!("Starting upload for file: {}", file);

            let client = self.client.lock().await.clone().unwrap();

            let tracker = Arc::new(ProgressTracker::new(
                1_000_000, // Example total size
                client,
                file.clone(),
            ));
            self.state
                .lock()
                .await
                .insert(file.clone(), tracker.clone());

            let state_clone = self.state.clone();
            tokio::spawn(async move {
                if let Err(e) = process_and_upload(tracker, state_clone).await {
                    error!("Upload failed for file {}: {:?}", file, e);
                }
            });
        } else if let Some(cmd) = payload.strip_prefix("cancel ") {
            let file = cmd.trim().to_string();
            self.state.lock().await.remove(&file);
            info!("Upload canceled for file: {}", file);
        } else {
            warn!("Unknown command received: {}", payload);
        }
    }
}
