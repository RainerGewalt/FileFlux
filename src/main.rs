
mod config;

use config::Config;

fn main() {
    match Config::from_env() {
        Ok(config) => {
            println!("Configuration loaded successfully:");
            println!("Instance ID: {}", config.instance_id);
            println!("MQTT Address: {}", config.mqtt_address);
            println!("Log Level: {}", config.log_level);
            // Optional: Debug-Mode anzeigen
            if config.debug_mode {
                println!("Debug mode is enabled.");
            }
        }
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
        }
    } 
}
