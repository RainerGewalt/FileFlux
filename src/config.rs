use dotenvy::dotenv;
use serde::Deserialize;
use std::env;
use thiserror::Error;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub mqtt_root_topic: String,
    pub mqtt_max_retries: i32,
    pub mqtt_retry_interval_ms: u64,

    // SMB Configuration
    pub smb_target_ip: String,
    pub smb_share_name: String,
    pub smb_target_folder: String,
    pub smb_username: String,
    pub smb_password: String,
    pub smb_connection_timeout_ms: u64,

    // SFTP Configuration
    pub sftp_host: String,
    pub sftp_port: u16,
    pub sftp_username: String,
    pub sftp_password: String,
    pub sftp_target_folder: String,
    pub sftp_connection_timeout_ms: u64,

    pub default_file_source: String,
    pub recursive_upload: bool,
    pub allow_compression: bool,
    pub compression_quality: u8,
    pub max_file_upload_size_mb: u32,
    pub file_filters: Vec<String>,
    pub upload_strategy: String,

    pub log_topic: String,
    pub status_topic: String,
    pub command_topic: String,
    pub progress_topic: String,
    pub analytics_topic: String,
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

        // Validate compression quality
        if !(0..=100).contains(&self.compression_quality) {
            return Err(ConfigError::ParsingError(
                "COMPRESSION_QUALITY must be between 0 and 100".to_string(),
            ));
        }

        Ok(())
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        dotenv().ok(); // Load environment variables from .env file

        let config = Self {
            // MQTT Configuration
            mqtt_host: env::var("MQTT_HOST").map_err(|_| ConfigError::MissingOrInvalid("MQTT_HOST".to_string()))?,
            mqtt_port: env::var("MQTT_PORT")
                .map_err(|_| ConfigError::MissingOrInvalid("MQTT_PORT".to_string()))?
                .parse::<u16>()
                .map_err(|_| ConfigError::ParsingError("MQTT_PORT must be a valid number".to_string()))?,
            mqtt_username: env::var("MQTT_USERNAME").unwrap_or_else(|_| "".to_string()), // Default to empty
            mqtt_password: env::var("MQTT_PASSWORD").unwrap_or_else(|_| "".to_string()), // Default to empty
            mqtt_root_topic: env::var("MQTT_ROOT_TOPIC").unwrap_or_else(|_| "image_uploader".to_string()),
            mqtt_max_retries: env::var("MQTT_MAX_RETRIES")
                .unwrap_or_else(|_| "-1".to_string())
                .parse::<i32>()
                .map_err(|_| ConfigError::ParsingError("MQTT_MAX_RETRIES must be an integer".to_string()))?,
            mqtt_retry_interval_ms: env::var("MQTT_RETRY_INTERVAL_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("MQTT_RETRY_INTERVAL_MS must be a valid number".to_string()))?,

            // SMB Configuration
            smb_target_ip: env::var("SMB_TARGET_IP").map_err(|_| ConfigError::MissingOrInvalid("SMB_TARGET_IP".to_string()))?,
            smb_share_name: env::var("SMB_SHARE_NAME").unwrap_or_else(|_| "default_share".to_string()),
            smb_target_folder: env::var("SMB_TARGET_FOLDER").map_err(|_| ConfigError::MissingOrInvalid("SMB_TARGET_FOLDER".to_string()))?,
            smb_username: env::var("SMB_USERNAME").unwrap_or_else(|_| "".to_string()), // Default to empty
            smb_password: env::var("SMB_PASSWORD").unwrap_or_else(|_| "".to_string()), // Default to empty
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
            sftp_username: env::var("SFTP_USERNAME").unwrap_or_else(|_| "".to_string()), // Default to empty
            sftp_password: env::var("SFTP_PASSWORD").unwrap_or_else(|_| "".to_string()), // Default to empty
            sftp_target_folder: env::var("SFTP_TARGET_FOLDER").unwrap_or_else(|_| "/remote/uploads".to_string()),
            sftp_connection_timeout_ms: env::var("SFTP_CONNECTION_TIMEOUT_MS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("SFTP_CONNECTION_TIMEOUT_MS must be a valid number".to_string()))?,

            // File Handling
            default_file_source: env::var("DEFAULT_FILE_SOURCE").unwrap_or_else(|_| "/data/uploads".to_string()),
            recursive_upload: env::var("RECURSIVE_UPLOAD")
                .unwrap_or_else(|_| "true".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("RECURSIVE_UPLOAD must be true or false".to_string()))?,
            allow_compression: env::var("ALLOW_COMPRESSION")
                .unwrap_or_else(|_| "false".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("ALLOW_COMPRESSION must be true or false".to_string()))?,
            compression_quality: env::var("COMPRESSION_QUALITY")
                .unwrap_or_else(|_| "80".to_string())
                .parse::<u8>()
                .map_err(|_| ConfigError::ParsingError("COMPRESSION_QUALITY must be a valid number".to_string()))?,
            max_file_upload_size_mb: env::var("MAX_FILE_UPLOAD_SIZE_MB")
                .unwrap_or_else(|_| "200".to_string())
                .parse::<u32>()
                .map_err(|_| ConfigError::ParsingError("MAX_FILE_UPLOAD_SIZE_MB must be a valid number".to_string()))?,
            file_filters: env::var("FILE_FILTERS")
                .unwrap_or_else(|_| "jpg,jpeg,png".to_string())
                .split(',')
                .map(|s| s.to_string())
                .collect(),
            upload_strategy: env::var("UPLOAD_STRATEGY").unwrap_or_else(|_| "batch".to_string()),


            // MQTT Topics
            log_topic: env::var("LOG_TOPIC").unwrap_or_else(|_| "logs".to_string()),
            status_topic: env::var("STATUS_TOPIC").unwrap_or_else(|_| "status".to_string()),
            command_topic: env::var("COMMAND_TOPIC").unwrap_or_else(|_| "commands".to_string()),
            progress_topic: env::var("PROGRESS_TOPIC").unwrap_or_else(|_| "progress".to_string()), // Added
            analytics_topic: env::var("ANALYTICS_TOPIC").unwrap_or_else(|_| "analytics".to_string()), // Added

        };

        // Validate timeouts after constructing the configuration
        config.validate_timeouts()?;

        Ok(config)
    }
}
