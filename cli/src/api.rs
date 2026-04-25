use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server_url: Option<String>,
    pub token: Option<String>,
    pub user_id: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: Some("http://localhost:8080".to_string()),
            token: None,
            user_id: None,
        }
    }
}

pub fn get_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zhiati");

    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }

    config_dir.join("config.json")
}

pub fn load_config() -> Config {
    let path = get_config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&content) {
                return config;
            }
        }
    }
    Config::default()
}

pub fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(config)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn get_config() -> Config {
    load_config()
}

pub async fn request<T: serde::de::DeserializeOwned>(
    server_url: &str,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> Result<T, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);

    let mut request = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        _ => return Err("Invalid HTTP method".into()),
    };

    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    if let Some(body) = body {
        request = request.json(&body);
    }

    let response = request.send().await?;
    let status = response.status();

    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, error_text).into());
    }

    let result = response.json::<T>().await?;
    Ok(result)
}

pub use zhiati_shared::{AuthResponse, ApiResponse, Note, CliNoteOutput};

pub async fn api_register(
    server_url: &str,
    email: &str,
    password: &str,
) -> Result<AuthResponse, Box<dyn std::error::Error>> {
    let response: ApiResponse<AuthResponse> = request(
        server_url,
        "POST",
        "/api/auth/register",
        None,
        Some(serde_json::json!({
            "email": email,
            "password": password
        })),
    )
    .await?;

    match response.data {
        Some(data) => Ok(data),
        None => Err(response.error.unwrap_or("Unknown error".to_string()).into()),
    }
}

pub async fn api_login(
    server_url: &str,
    email: &str,
    password: &str,
) -> Result<AuthResponse, Box<dyn std::error::Error>> {
    let response: ApiResponse<AuthResponse> = request(
        server_url,
        "POST",
        "/api/auth/login",
        None,
        Some(serde_json::json!({
            "email": email,
            "password": password
        })),
    )
    .await?;

    match response.data {
        Some(data) => Ok(data),
        None => Err(response.error.unwrap_or("Unknown error".to_string()).into()),
    }
}

pub async fn api_list_notes(
    server_url: &str,
    token: &str,
) -> Result<Vec<Note>, Box<dyn std::error::Error>> {
    let response: ApiResponse<Vec<Note>> = request(
        server_url,
        "GET",
        "/api/notes",
        Some(token),
        None,
    )
    .await?;

    match response.data {
        Some(data) => Ok(data),
        None => Err(response.error.unwrap_or("Unknown error".to_string()).into()),
    }
}

pub async fn api_create_note(
    server_url: &str,
    token: &str,
    title: &str,
    content: Option<&str>,
) -> Result<Note, Box<dyn std::error::Error>> {
    let response: ApiResponse<Note> = request(
        server_url,
        "POST",
        "/api/notes",
        Some(token),
        Some(serde_json::json!({
            "title": title,
            "content": content
        })),
    )
    .await?;

    match response.data {
        Some(data) => Ok(data),
        None => Err(response.error.unwrap_or("Unknown error".to_string()).into()),
    }
}

pub async fn api_get_note(
    server_url: &str,
    token: &str,
    id: &str,
) -> Result<Note, Box<dyn std::error::Error>> {
    let response: ApiResponse<Note> = request(
        server_url,
        "GET",
        &format!("/api/notes/{}", id),
        Some(token),
        None,
    )
    .await?;

    match response.data {
        Some(data) => Ok(data),
        None => Err(response.error.unwrap_or("Unknown error".to_string()).into()),
    }
}

pub async fn api_update_note(
    server_url: &str,
    token: &str,
    id: &str,
    title: Option<&str>,
    content: Option<&str>,
) -> Result<Note, Box<dyn std::error::Error>> {
    let response: ApiResponse<Note> = request(
        server_url,
        "PUT",
        &format!("/api/notes/{}", id),
        Some(token),
        Some(serde_json::json!({
            "title": title,
            "content": content
        })),
    )
    .await?;

    match response.data {
        Some(data) => Ok(data),
        None => Err(response.error.unwrap_or("Unknown error".to_string()).into()),
    }
}

pub async fn api_delete_note(
    server_url: &str,
    token: &str,
    id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let response: ApiResponse<()> = request(
        server_url,
        "DELETE",
        &format!("/api/notes/{}", id),
        Some(token),
        None,
    )
    .await?;

    if response.success {
        Ok(())
    } else {
        Err(response.error.unwrap_or("Unknown error".to_string()).into())
    }
}
