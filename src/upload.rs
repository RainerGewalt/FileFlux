use crate::progress_tracker::{ProgressTracker, SharedState};
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use std::error::Error;
use tracing::info;

pub async fn process_and_upload(
    tracker: Arc<ProgressTracker>,
    state: SharedState,
) -> Result<(), Box<dyn Error>> {
    // Simulierte Bildverarbeitung
    sleep(Duration::from_secs(1)).await;

    // Simulierter Upload mit Fortschrittsaktualisierung
    for _ in 0..10 {
        sleep(Duration::from_millis(500)).await;
        {
            let mut uploaded = tracker.uploaded.lock().await;
            *uploaded += 100_000; // Simulierte Bytes
        }
        tracker.update_progress().await;
    }

    info!("Upload abgeschlossen für Datei: {}", tracker.file_name);

    // Upload abgeschlossen, Fortschritt entfernen
    state.lock().await.remove(&tracker.file_name);

    Ok(())
}
