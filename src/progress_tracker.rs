use rumqttc::{AsyncClient, QoS};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use log::{error, info};
use std::fmt;

pub type SharedState = Arc<Mutex<HashMap<String, Arc<ProgressTracker>>>>;

pub struct ProgressTracker {
    total_size: Mutex<u64>,    // Mutex to protect access to total_size
    uploaded_size: Mutex<u64>, // Mutex to protect access to uploaded_size
    client: AsyncClient,       // MQTT client for publishing progress
    task_id: String,           // Unique identifier for the task
}

impl ProgressTracker {
    pub fn new(total_size: u64, client: AsyncClient, task_id: String) -> Self {
        Self {
            total_size: Mutex::new(total_size), // Wrap the total_size in Mutex::new
            uploaded_size: Mutex::new(0),      // Initialize uploaded_size with Mutex::new(0)
            client,
            task_id,
        }
    }

    pub async fn set_total_size(&self, size: u64) {
        let mut total_size = self.total_size.lock().await;
        *total_size = size;
        info!(
            "Set total size for task {}: {} bytes",
            self.task_id, size
        );
    }

    pub async fn update_progress(&self, bytes_uploaded: u64) {
        let mut uploaded_size = self.uploaded_size.lock().await;
        let total_size = *self.total_size.lock().await;
        *uploaded_size += bytes_uploaded;

        let progress = if total_size > 0 {
            (*uploaded_size as f64 / total_size as f64) * 100.0
        } else {
            0.0
        };

        info!(
            "Progress update for task {}: {:.2}% uploaded",
            self.task_id, progress
        );

        // Publish progress update over MQTT
        let progress_topic = format!("progress/{}", self.task_id);
        let progress_message = format!("{{\"progress\": {:.2}}}", progress);

        if let Err(e) = self
            .client
            .publish(progress_topic, QoS::AtLeastOnce, false, progress_message)
            .await
        {
            error!("Failed to publish progress update: {:?}", e);
        }
    }
}

// Manual implementation of Debug for ProgressTracker
impl fmt::Debug for ProgressTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressTracker")
            .field("total_size", &"Mutex<u64>")
            .field("uploaded_size", &"Mutex<u64>")
            .field("task_id", &self.task_id)
            .finish()
    }
}
