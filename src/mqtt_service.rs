use crate::progress_tracker::{ProgressTracker, SharedState};
use crate::upload::process_and_upload;
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Debug)]
enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

pub struct MqttService {
    client_state: ClientState,
    client: Option<AsyncClient>,
    eventloop: Option<EventLoop>,
    state: SharedState,
}

impl MqttService {
    pub fn new(state: SharedState) -> Self {
        Self {
            client_state: ClientState::Disconnected,
            client: None,
            eventloop: None,
            state,
        }
    }

    pub async fn start(&mut self, mqtt_host: &str, mqtt_port: u16, mqtt_client_id: &str) {
        info!("Starte MQTT-Service...");
        debug!("Konfiguriere Broker unter {}:{}", mqtt_host, mqtt_port);

        let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
        mqtt_options.set_keep_alive(Duration::from_secs(10)); // Heartbeat alle 10 Sekunden
        mqtt_options.set_clean_session(true); // Alte Sitzungen löschen

        let (client, eventloop) = AsyncClient::new(mqtt_options, 10);
        self.client = Some(client.clone());
        self.eventloop = Some(eventloop);

        loop {
            match self.connect_and_subscribe("upload/control").await {
                Ok(_) => {
                    self.client_state = ClientState::Connected;
                    info!("Verbindung zum Broker hergestellt. Erfolgreich abonniert.");

                    if let Some(mut eventloop) = self.eventloop.take() {
                        while let Ok(event) = eventloop.poll().await {
                            self.handle_event(event).await;
                        }
                        self.eventloop = Some(eventloop);
                    }

                    warn!("Verbindung verloren. Erneuter Verbindungsversuch...");
                    self.client_state = ClientState::Disconnected;
                }
                Err(e) => {
                    error!("Fehler bei Verbindung oder Abonnement: {}", e);
                    self.client_state = ClientState::Error(e);
                    sleep(Duration::from_secs(5)).await; // Warte 5 Sekunden vor erneutem Versuch
                }
            }
        }
    }

    async fn connect_and_subscribe(&mut self, topic: &str) -> Result<(), String> {
        self.client_state = ClientState::Connecting;
        debug!("Versuche, eine Verbindung herzustellen und zu abonnieren.");

        // Warte auf erfolgreiche Verbindung
        self.wait_for_connection()
            .await
            .map_err(|e| format!("Verbindungsfehler: {}", e))?;

        debug!("Verbunden. Versuche, Topic '{}' zu abonnieren.", topic);

        // Topic abonnieren
        if let Some(client) = &self.client {
            client
                .subscribe(topic, QoS::AtLeastOnce)
                .await
                .map_err(|e| format!("Abonnement fehlgeschlagen für '{}': {}", topic, e))?;
            info!("Erfolgreich abonniert: '{}'.", topic);
            Ok(())
        } else {
            Err("Client nicht initialisiert".into())
        }
    }

    async fn wait_for_connection(&mut self) -> Result<(), String> {
        if let Some(eventloop) = &mut self.eventloop {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(incoming)) => {
                        debug!("Eingehendes Ereignis: {:?}", incoming);
                        if let Packet::ConnAck(connack) = incoming {
                            if connack.code == rumqttc::ConnectReturnCode::Success {
                                break;
                            } else {
                                return Err(format!("Verbindung fehlgeschlagen, Code: {:?}", connack.code));
                            }
                        }
                    }
                    Ok(Event::Outgoing(_)) => {
                        debug!("Ausgehendes Paket verarbeitet.");
                    }
                    Err(e) => {
                        return Err(format!("Verbindungsfehler: {:?}", e));
                    }
                }
            }
            Ok(())
        } else {
            Err("EventLoop nicht initialisiert".into())
        }
    }

    async fn handle_event(&self, event: Event) {
        if let Event::Incoming(Packet::Publish(publish)) = event {
            let topic = publish.topic;
            let payload = String::from_utf8(publish.payload.to_vec()).unwrap_or_else(|_| "".to_string());

            match topic.as_str() {
                "upload/control" => {
                    self.handle_upload_command(payload).await;
                }
                _ => {
                    warn!("Unbekanntes Topic: {}", topic);
                }
            }
        }
    }

    async fn handle_upload_command(&self, payload: String) {
        match payload.as_str() {
            cmd if cmd.starts_with("start") => {
                if let Some(file) = cmd.strip_prefix("start ") {
                    info!("Starte Upload für Datei: {}", file);

                    let tracker = Arc::new(ProgressTracker::new(
                        1_000_000, // Gesamtgröße des Uploads (Dummy-Wert)
                        self.client.clone().unwrap(),
                        file.to_string(),
                    ));
                    self.state.lock().await.insert(file.to_string(), tracker.clone());

                    let state_clone = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = process_and_upload(tracker, state_clone).await {
                            error!("Upload fehlgeschlagen für Datei {}: {:?}", file, e);
                        }
                    });
                }
            }
            cmd if cmd.starts_with("cancel") => {
                if let Some(file) = cmd.strip_prefix("cancel ") {
                    self.state.lock().await.remove(file);
                    info!("Upload abgebrochen für Datei: {}", file);
                }
            }
            _ => {
                warn!("Unbekannter Befehl: {}", payload);
            }
        }
    }
}
