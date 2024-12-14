use crate::mqtt_service::MqttService;
use crate::service_utils::publish_progress;
use log::info;
use std::fmt;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering}; // Import AtomicBool and Ordering
use tokio::sync::Mutex;

pub type SharedState = Arc<Mutex<HashMap<String, Arc<ProgressTracker>>>>;

pub struct ProgressTracker {
    pub(crate) total_size: Mutex<u64>,
    pub(crate) uploaded_size: Mutex<u64>,
    mqtt_service: Arc<MqttService>,
    pub task_id: String,  // Make task_id public if needed elsewhere
    pub cancelled: AtomicBool, // Add the cancelled field
}

impl ProgressTracker {
    pub fn new(
        total_size: u64,
        mqtt_service: Arc<MqttService>, // Expects Arc<MqttService>
        task_id: String,
    ) -> Self {
        Self {
            total_size: Mutex::new(total_size),
            uploaded_size: Mutex::new(0),
            mqtt_service,
            task_id,
            cancelled: AtomicBool::new(false), // Initialize as not cancelled
        }
    }

    /// Check if the task is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Stop the progress tracker
    pub async fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        info!("Progress tracker for task {} marked as stopped.", self.task_id);

        // Optionally publish a progress update or a cancellation event
        publish_progress(self.mqtt_service.clone(), 0, 0); // Example reset progress
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
        if self.is_cancelled() {
            info!("Task {} has been cancelled. Skipping progress update.", self.task_id);
            return;
        }

        let mut uploaded_size = self.uploaded_size.lock().await;
        let total_size = *self.total_size.lock().await;
        *uploaded_size += bytes_uploaded;

        let progress_percentage = if total_size > 0 {
            (*uploaded_size as f64 / total_size as f64) * 100.0
        } else {
            0.0
        };

        info!(
            "Progress update for task {}: {:.2}% uploaded",
            self.task_id, progress_percentage
        );

        // Use the publish_progress method
        publish_progress(
            self.mqtt_service.clone(),
            *uploaded_size,
            total_size,
        );
    }
}

// Implement Debug for ProgressTracker
impl fmt::Debug for ProgressTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressTracker")
            .field("total_size", &"Mutex<u64>")
            .field("uploaded_size", &"Mutex<u64>")
            .field("task_id", &self.task_id)
            .field("cancelled", &self.cancelled.load(Ordering::SeqCst)) // Include the cancellation status
            .finish()
    }
}
