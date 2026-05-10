use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_server_url")]
    pub server_url: String,
    pub token: Option<String>,
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    #[serde(default)]
    pub local_email: Option<String>,
    #[serde(default)]
    pub local_password_encrypted: Option<String>,
    #[serde(default = "default_true")]
    pub cloud_enabled: bool,
}

fn default_server_url() -> String {
    "http://localhost:8080".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server_url: default_server_url(),
            token: None,
            user_id: None,
            user_email: None,
            local_email: None,
            local_password_encrypted: None,
            cloud_enabled: true,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zhiati")
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load_config() -> Config {
    let path = config_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    } else {
        Config::default()
    }
}

pub fn save_config(config: &Config) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(config_path(), content).map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}
