use rumqttc::{AsyncClient, QoS};
use std::sync::Arc;
use tokio::sync::Mutex;

pub type SharedState = Arc<Mutex<std::collections::HashMap<String, Arc<ProgressTracker>>>>;

#[derive(Debug)]
pub struct ProgressTracker {
    pub uploaded: Arc<Mutex<u64>>,
    pub total: u64,
    mqtt_client: AsyncClient,
    pub(crate) file_name: String,
}

impl ProgressTracker {
    pub fn new(total: u64, mqtt_client: AsyncClient, file_name: String) -> Self {
        Self {
            uploaded: Arc::new(Mutex::new(0)),
            total,
            mqtt_client,
            file_name,
        }
    }

    pub async fn update_progress(&self) {
        let uploaded = *self.uploaded.lock().await;
        let progress = (uploaded as f64 / self.total as f64) * 100.0;

        let topic = format!("upload/progress/{}", self.file_name);
        let payload = format!("{{\"progress\": {:.2}}}", progress);

        if let Err(e) = self
            .mqtt_client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
        {
            eprintln!("Fehler beim Senden des Fortschritts: {:?}", e);
        }
    }
}
