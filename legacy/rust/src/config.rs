use dotenvy::dotenv;
use serde::Deserialize;
use std::env;
use thiserror::Error;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Identifies this worker instance in every MQTT topic it uses.
    pub worker_id: String,

    // MQTT
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub mqtt_max_retries: i32,
    pub mqtt_retry_interval_ms: u64,
    pub mqtt_root_topic: String,

    /// Path to the rclone binary. rclone is the transfer engine; TrailTransfer
    /// only builds jobs, config and reporting around it.
    pub rclone_bin: String,

    // SMB target — used to build the rclone `smb` remote for this worker.
    pub smb_target_ip: String,
    pub smb_share_name: String,
    pub smb_target_folder: String,
    pub smb_username: String,
    pub smb_password: String,
    pub smb_connection_timeout_ms: u64,

    // SFTP target — used to build the rclone `sftp` remote for this worker.
    pub sftp_host: String,
    pub sftp_port: u16,
    pub sftp_username: String,
    pub sftp_password: String,
    pub sftp_target_folder: String,
    pub sftp_connection_timeout_ms: u64,

    pub max_file_transfer_size_mb: u32,
    pub file_filters: Vec<String>,
    pub transfer_strategy: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Environment variable {0} is missing or invalid.")]
    MissingOrInvalid(String),
    #[error("Parsing error: {0}")]
    ParsingError(String),
}

impl Config {
    /// Validate timeout values and other critical configurations.
    fn validate_timeouts(&self) -> Result<(), ConfigError> {
        const MIN_TIMEOUT: u64 = 100;
        const MAX_TIMEOUT: u64 = 1_000_000;

        if !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&self.mqtt_retry_interval_ms) {
            return Err(ConfigError::ParsingError(format!(
                "MQTT_RETRY_INTERVAL_MS must be between {} and {} ms",
                MIN_TIMEOUT, MAX_TIMEOUT
            )));
        }
        if !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&self.smb_connection_timeout_ms) {
            return Err(ConfigError::ParsingError(format!(
                "SMB_CONNECTION_TIMEOUT_MS must be between {} and {} ms",
                MIN_TIMEOUT, MAX_TIMEOUT
            )));
        }
        if !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&self.sftp_connection_timeout_ms) {
            return Err(ConfigError::ParsingError(format!(
                "SFTP_CONNECTION_TIMEOUT_MS must be between {} and {} ms",
                MIN_TIMEOUT, MAX_TIMEOUT
            )));
        }

        Ok(())
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        dotenv().ok(); // Load environment variables from .env file

        let mqtt_root_topic = env::var("MQTT_ROOT_TOPIC").unwrap_or_else(|_| "trailtransfer".to_string());
        let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| "worker-1".to_string());

        let config = Self {
            worker_id,
            mqtt_root_topic,

            // MQTT Configuration
            mqtt_host: env::var("MQTT_HOST").map_err(|_| ConfigError::MissingOrInvalid("MQTT_HOST".to_string()))?,
            mqtt_port: env::var("MQTT_PORT")
                .map_err(|_| ConfigError::MissingOrInvalid("MQTT_PORT".to_string()))?
                .parse::<u16>()
                .map_err(|_| ConfigError::ParsingError("MQTT_PORT must be a valid number".to_string()))?,
            mqtt_username: env::var("MQTT_USERNAME").unwrap_or_default(),
            mqtt_password: env::var("MQTT_PASSWORD").unwrap_or_default(),
            mqtt_max_retries: env::var("MQTT_MAX_RETRIES")
                .unwrap_or_else(|_| "-1".to_string())
                .parse::<i32>()
                .map_err(|_| ConfigError::ParsingError("MQTT_MAX_RETRIES must be an integer".to_string()))?,
            mqtt_retry_interval_ms: env::var("MQTT_RETRY_INTERVAL_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("MQTT_RETRY_INTERVAL_MS must be a valid number".to_string()))?,

            rclone_bin: env::var("RCLONE_BIN").unwrap_or_else(|_| "rclone".to_string()),

            // SMB Configuration
            smb_target_ip: env::var("SMB_TARGET_IP").map_err(|_| ConfigError::MissingOrInvalid("SMB_TARGET_IP".to_string()))?,
            smb_share_name: env::var("SMB_SHARE_NAME").unwrap_or_else(|_| "default_share".to_string()),
            smb_target_folder: env::var("SMB_TARGET_FOLDER").map_err(|_| ConfigError::MissingOrInvalid("SMB_TARGET_FOLDER".to_string()))?,
            smb_username: env::var("SMB_USERNAME").unwrap_or_default(),
            smb_password: env::var("SMB_PASSWORD").unwrap_or_default(),
            smb_connection_timeout_ms: env::var("SMB_CONNECTION_TIMEOUT_MS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("SMB_CONNECTION_TIMEOUT_MS must be a valid number".to_string()))?,

            // SFTP Configuration
            sftp_host: env::var("SFTP_HOST").map_err(|_| ConfigError::MissingOrInvalid("SFTP_HOST".to_string()))?,
            sftp_port: env::var("SFTP_PORT")
                .unwrap_or_else(|_| "22".to_string())
                .parse::<u16>()
                .map_err(|_| ConfigError::ParsingError("SFTP_PORT must be a valid number".to_string()))?,
            sftp_username: env::var("SFTP_USERNAME").unwrap_or_default(),
            sftp_password: env::var("SFTP_PASSWORD").unwrap_or_default(),
            sftp_target_folder: env::var("SFTP_TARGET_FOLDER").unwrap_or_else(|_| "/remote/uploads".to_string()),
            sftp_connection_timeout_ms: env::var("SFTP_CONNECTION_TIMEOUT_MS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("SFTP_CONNECTION_TIMEOUT_MS must be a valid number".to_string()))?,

            // Job defaults (overridable per job via MQTT command)
            max_file_transfer_size_mb: env::var("MAX_FILE_TRANSFER_SIZE_MB")
                .unwrap_or_else(|_| "200".to_string())
                .parse::<u32>()
                .map_err(|_| ConfigError::ParsingError("MAX_FILE_TRANSFER_SIZE_MB must be a valid number".to_string()))?,
            file_filters: env::var("FILE_FILTERS")
                .unwrap_or_else(|_| "jpg,jpeg,png".to_string())
                .split(',')
                .map(|s| s.to_string())
                .collect(),
            transfer_strategy: env::var("TRANSFER_STRATEGY").unwrap_or_else(|_| "batch".to_string()),
        };

        config.validate_timeouts()?;

        Ok(config)
    }

    pub fn command_topic(&self) -> String {
        format!("{}/{}/commands", self.mqtt_root_topic, self.worker_id)
    }

    pub fn health_topic(&self) -> String {
        format!("{}/{}/health", self.mqtt_root_topic, self.worker_id)
    }

    pub fn capabilities_topic(&self) -> String {
        format!("{}/{}/capabilities", self.mqtt_root_topic, self.worker_id)
    }

    fn job_topic(&self, job_id: &str, suffix: &str) -> String {
        format!("{}/{}/jobs/{}/{}", self.mqtt_root_topic, self.worker_id, job_id, suffix)
    }

    pub fn job_status_topic(&self, job_id: &str) -> String {
        self.job_topic(job_id, "status")
    }

    pub fn job_progress_topic(&self, job_id: &str) -> String {
        self.job_topic(job_id, "progress")
    }

    pub fn job_result_topic(&self, job_id: &str) -> String {
        self.job_topic(job_id, "result")
    }

    pub fn job_logs_topic(&self, job_id: &str) -> String {
        self.job_topic(job_id, "logs")
    }
}
