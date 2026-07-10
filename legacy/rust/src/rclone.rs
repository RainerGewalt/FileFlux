// rclone.rs
//
// TrailTransfer never talks SMB/SFTP wire protocol itself. This module's only
// job is to (1) render a per-worker rclone config from TrailTransfer's own
// config, and (2) run `rclone copyto` per file, turning its JSON stats stream
// into progress callbacks. rclone stays the transfer engine end to end.

use crate::config::Config;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKind {
    Smb,
    Sftp,
}

impl RemoteKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "smb" => Some(RemoteKind::Smb),
            "sftp" => Some(RemoteKind::Sftp),
            _ => None,
        }
    }

    fn remote_name(&self) -> &'static str {
        match self {
            RemoteKind::Smb => "smb_remote",
            RemoteKind::Sftp => "sftp_remote",
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TransferOutcome {
    pub bytes_transferred: u64,
    pub duration: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum RcloneError {
    #[error("failed to run rclone: {0}")]
    Io(#[from] io::Error),
    #[error("rclone exited with a non-zero status ({0})")]
    NonZeroExit(String),
}

/// Obscures a plaintext secret using `rclone obscure`, as required by rclone
/// config files. Returns an empty string for empty input.
async fn obscure(rclone_bin: &str, plaintext: &str) -> Result<String, RcloneError> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }

    let output = Command::new(rclone_bin)
        .arg("obscure")
        .arg(plaintext)
        .output()
        .await?;

    if !output.status.success() {
        return Err(RcloneError::NonZeroExit(
            "rclone obscure failed".to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Writes a per-worker rclone config file describing the `smb_remote` and
/// `sftp_remote` remotes derived from TrailTransfer's own configuration.
/// Written with 0600 permissions and never logged.
pub async fn write_rclone_config(config: &Config) -> Result<PathBuf, RcloneError> {
    let smb_pass = obscure(&config.rclone_bin, &config.smb_password).await?;
    let sftp_pass = obscure(&config.rclone_bin, &config.sftp_password).await?;

    let contents = format!(
        "[smb_remote]\n\
type = smb\n\
host = {smb_host}\n\
user = {smb_user}\n\
pass = {smb_pass}\n\
port = 445\n\
\n\
[sftp_remote]\n\
type = sftp\n\
host = {sftp_host}\n\
port = {sftp_port}\n\
user = {sftp_user}\n\
pass = {sftp_pass}\n",
        smb_host = config.smb_target_ip,
        smb_user = config.smb_username,
        sftp_host = config.sftp_host,
        sftp_port = config.sftp_port,
        sftp_user = config.sftp_username,
    );

    let path = std::env::temp_dir().join(format!("trailtransfer-{}.rclone.conf", config.worker_id));
    fs::write(&path, contents).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await?;
    }

    Ok(path)
}

fn remote_path(config: &Config, kind: RemoteKind, destination_path: &str) -> String {
    let destination_path = destination_path.trim_start_matches('/');
    match kind {
        RemoteKind::Smb => format!(
            "smb_remote:{}/{}/{}",
            config.smb_share_name.trim_matches('/'),
            config.smb_target_folder.trim_matches('/'),
            destination_path
        ),
        RemoteKind::Sftp => format!(
            "sftp_remote:{}/{}",
            config.sftp_target_folder.trim_matches('/'),
            destination_path
        ),
    }
}

/// Extracts the running byte count from one line of `rclone --use-json-log
/// --stats` output. Non-stats lines (plain log messages) yield None.
fn parse_stats_bytes(line: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.get("stats")?.get("bytes")?.as_u64()
}

/// Runs a single file transfer through rclone, streaming progress via
/// `on_progress` as rclone reports incremental byte counts.
pub async fn transfer_file(
    config: &Config,
    rclone_config_path: &Path,
    kind: RemoteKind,
    source_path: &str,
    destination_path: &str,
    mut on_progress: impl FnMut(u64) + Send,
) -> Result<TransferOutcome, RcloneError> {
    let target = remote_path(config, kind, destination_path);
    let start = Instant::now();

    let mut child = Command::new(&config.rclone_bin)
        .arg("copyto")
        .arg(source_path)
        .arg(&target)
        .arg("--config")
        .arg(rclone_config_path)
        .arg("--use-json-log")
        .arg("--stats")
        .arg("500ms")
        .arg("--stats-log-level")
        .arg("NOTICE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr = child.stderr.take().expect("stderr was piped");
    let mut lines = BufReader::new(stderr).lines();
    let mut bytes_transferred = 0u64;

    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(bytes) = parse_stats_bytes(&line) {
            bytes_transferred = bytes;
            on_progress(bytes);
        }
    }

    let status = child.wait().await?;
    let duration = start.elapsed();

    if !status.success() {
        return Err(RcloneError::NonZeroExit(format!(
            "rclone copyto {} -> {} exited with {:?}",
            source_path,
            target,
            status.code()
        )));
    }

    Ok(TransferOutcome {
        bytes_transferred,
        duration,
    })
}
