use dotenvy::dotenv;
use serde::Deserialize;
use std::env;
use thiserror::Error;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub mqtt_root_topic: String,
    pub mqtt_max_retries: i32,
    pub mqtt_retry_interval_ms: u64,

    pub smb_target_ip: String,
    pub smb_share_name: String,
    pub smb_target_folder: String,
    pub smb_connection_timeout_ms: u64,

    pub default_file_source: String,
    pub recursive_upload: bool,
    pub allow_compression: bool,
    pub compression_quality: u8,
    pub max_file_upload_size_mb: u32,
    pub file_filters: Vec<String>,
    pub upload_strategy: String,

    pub log_level: String,
    pub log_topic: String,
    pub status_topic: String,
    pub command_topic: String,

    pub healthcheck_interval_ms: u64,
    pub connection_timeout_ms: u64,

    pub debug_mode: bool,
    pub enable_profiling: bool,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Environment variable {0} is missing or invalid.")]
    MissingOrInvalid(String),
    #[error("Parsing error: {0}")]
    ParsingError(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenv().ok();

        Ok(Self {
            // MQTT Configuration
            mqtt_host: env::var("MQTT_HOST").map_err(|_| ConfigError::MissingOrInvalid("MQTT_HOST".to_string()))?,
            mqtt_port: env::var("MQTT_PORT")
                .map_err(|_| ConfigError::MissingOrInvalid("MQTT_PORT".to_string()))?
                .parse::<u16>()
                .map_err(|_| ConfigError::ParsingError("MQTT_PORT must be a valid number".to_string()))?,
            mqtt_username: env::var("MQTT_USERNAME").map_err(|_| ConfigError::MissingOrInvalid("MQTT_USERNAME".to_string()))?,
            mqtt_password: env::var("MQTT_PASSWORD").map_err(|_| ConfigError::MissingOrInvalid("MQTT_PASSWORD".to_string()))?,
            mqtt_root_topic: env::var("MQTT_ROOT_TOPIC").unwrap_or_else(|_| "image_uploader".to_string()),
            mqtt_max_retries: env::var("MQTT_MAX_RETRIES")
                .unwrap_or_else(|_| "-1".to_string())
                .parse::<i32>()
                .map_err(|_| ConfigError::ParsingError("MQTT_MAX_RETRIES must be an integer".to_string()))?,
            mqtt_retry_interval_ms: env::var("MQTT_RETRY_INTERVAL_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("MQTT_RETRY_INTERVAL_MS must be a valid number".to_string()))?,

            // SMB Default Configuration
            smb_target_ip: env::var("SMB_TARGET_IP").map_err(|_| ConfigError::MissingOrInvalid("SMB_TARGET_IP".to_string()))?,
            smb_share_name: env::var("SMB_SHARE_NAME").unwrap_or_else(|_| "default_share".to_string()),
            smb_target_folder: env::var("SMB_TARGET_FOLDER").map_err(|_| ConfigError::MissingOrInvalid("SMB_TARGET_FOLDER".to_string()))?,
            smb_connection_timeout_ms: env::var("SMB_CONNECTION_TIMEOUT_MS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("SMB_CONNECTION_TIMEOUT_MS must be a valid number".to_string()))?,

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

            // Logging and Status Reporting
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            log_topic: env::var("LOG_TOPIC").unwrap_or_else(|_| "logs".to_string()),
            status_topic: env::var("STATUS_TOPIC").unwrap_or_else(|_| "status".to_string()),
            command_topic: env::var("COMMAND_TOPIC").unwrap_or_else(|_| "commands".to_string()),

            // Health Checks
            healthcheck_interval_ms: env::var("HEALTHCHECK_INTERVAL_MS")
                .unwrap_or_else(|_| "30000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("HEALTHCHECK_INTERVAL_MS must be a valid number".to_string()))?,
            connection_timeout_ms: env::var("CONNECTION_TIMEOUT_MS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("CONNECTION_TIMEOUT_MS must be a valid number".to_string()))?,

            // Debugging and Development
            debug_mode: env::var("DEBUG_MODE")
                .unwrap_or_else(|_| "false".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("DEBUG_MODE must be true or false".to_string()))?,
            enable_profiling: env::var("ENABLE_PROFILING")
                .unwrap_or_else(|_| "false".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("ENABLE_PROFILING must be true or false".to_string()))?,
        })
    }
}
