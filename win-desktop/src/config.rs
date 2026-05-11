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
    #[serde(default)]
    pub bound_cloud_email: Option<String>,
    #[serde(default = "default_true")]
    pub cloud_enabled: bool,
    /// Root directory for local attachment storage. Markdown stores relative paths from here.
    #[serde(default)]
    pub attachments_root: Option<String>,
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
            bound_cloud_email: None,
            cloud_enabled: true,
            attachments_root: None, // None means use default
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zhiati")
}

/// Default attachments root: {config_dir}/
/// Actual attachments live in {config_dir}/attachments/, and markdown stores
/// relative paths like 'attachments/{note_id}/{uuid}.ext', so the root should be config_dir().
fn default_attachments_root() -> PathBuf {
    config_dir()
}

/// Get the attachments root directory. Returns configured path or default.
pub fn attachments_root() -> PathBuf {
    let cfg = load_config();
    cfg.attachments_root
        .map(PathBuf::from)
        .unwrap_or_else(default_attachments_root)
}

/// Ensure attachments root exists, return absolute path
pub fn ensure_attachments_root() -> PathBuf {
    let dir = attachments_root();
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Migrate attachments from old root to new root. Moves all files.
pub fn migrate_attachments(old_root: PathBuf, new_root: PathBuf) -> Result<(), String> {
    if !old_root.exists() || !old_root.is_dir() {
        return Ok(()); // nothing to migrate
    }
    fs::create_dir_all(&new_root)
        .map_err(|e| format!("创建目标目录失败: {}", e))?;

    // Move all note subdirectories
    for entry in fs::read_dir(&old_root)
        .map_err(|e| format!("读取源目录失败: {}", e))?
    {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let src = entry.path();
            let dst = new_root.join(entry.file_name());
            if dst.exists() {
                // Merge: copy files that don't exist in destination
                if let Ok(files) = fs::read_dir(&src) {
                    for f in files.flatten() {
                        let dest_file = dst.join(f.file_name());
                        if !dest_file.exists() {
                            let _ = fs::copy(f.path(), &dest_file);
                        }
                    }
                }
                let _ = fs::remove_dir_all(&src);
            } else {
                fs::rename(&src, &dst)
                    .map_err(|e| format!("移动目录失败 {:?}: {}", src, e))?;
            }
        }
    }
    // Remove old root if empty
    let _ = fs::remove_dir(&old_root);
    Ok(())
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
