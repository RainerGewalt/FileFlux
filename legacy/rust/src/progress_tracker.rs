use crate::mqtt_service::MqttService;
use crate::service_utils::publish_job_progress;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

pub type SharedState = Arc<Mutex<HashMap<String, Arc<ProgressTracker>>>>;

pub struct ProgressTracker {
    pub(crate) total_size: Mutex<u64>,
    pub(crate) transferred_size: Mutex<u64>,
    mqtt_service: Arc<MqttService>,
    pub job_id: String,
    pub cancelled: AtomicBool,
}

impl ProgressTracker {
    pub fn new(total_size: u64, mqtt_service: Arc<MqttService>, job_id: String) -> Self {
        Self {
            total_size: Mutex::new(total_size),
            transferred_size: Mutex::new(0),
            mqtt_service,
            job_id,
            cancelled: AtomicBool::new(false),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Marks the job as cancelled. In-flight rclone processes finish their
    /// current file; no further files are started.
    pub async fn stop(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        info!("Progress tracker for job {} marked as stopped.", self.job_id);
        publish_job_progress(self.mqtt_service.clone(), &self.job_id, 0, 0);
    }

    pub async fn set_total_size(&self, size: u64) {
        let mut total_size = self.total_size.lock().await;
        *total_size = size;
        info!("Set total size for job {}: {} bytes", self.job_id, size);
    }

    /// Adds a byte delta to the job's transferred count and publishes
    /// progress. Delta-based so concurrent (batch-mode) file transfers can
    /// report into the same tracker safely.
    pub async fn add_transferred(&self, bytes: u64) {
        if self.is_cancelled() {
            return;
        }

        let (transferred, total) = {
            let mut current = self.transferred_size.lock().await;
            *current += bytes;
            (*current, *self.total_size.lock().await)
        };

        publish_job_progress(self.mqtt_service.clone(), &self.job_id, transferred, total);
    }
}

impl fmt::Debug for ProgressTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressTracker")
            .field("job_id", &self.job_id)
            .field("cancelled", &self.cancelled.load(Ordering::SeqCst))
            .finish()
    }
}
