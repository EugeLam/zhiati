use crate::commands::AppState;
use crate::config;
use serde::Serialize;
use shared::{ApiResponse, AuthResponse, LoginRequest, RegisterRequest};
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize)]
pub struct AuthResult {
    pub token: String,
    pub user_id: String,
    pub email: String,
}

#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<AuthResult, String> {
    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/auth/login", server_url))
        .json(&LoginRequest { email, password })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<AuthResponse> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "登录失败".into()));
    }

    let body: ApiResponse<AuthResponse> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let auth = body.data.ok_or_else(|| "登录失败: 无效响应".to_string())?;

    {
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = Some(auth.user.id.to_string());
    }
    {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = Some(auth.token.clone());
    }

    let c = config::Config {
        server_url: server_url.clone(),
        token: Some(auth.token.clone()),
        user_id: Some(auth.user.id.to_string()),
        user_email: Some(auth.user.email.clone()),
    };
    config::save_config(&c).map_err(|e| format!("保存配置失败: {}", e))?;

    // Emit auth-changed event so mini window can react
    let _ = app.emit("auth-changed", true);

    Ok(AuthResult {
        token: auth.token,
        user_id: auth.user.id.to_string(),
        email: auth.user.email,
    })
}

#[tauri::command]
pub async fn register(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<AuthResult, String> {
    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/auth/register", server_url))
        .json(&RegisterRequest {
            email: email.clone(),
            password,
        })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<AuthResponse> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "注册失败".into()));
    }

    let body: ApiResponse<AuthResponse> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let auth = body.data.ok_or_else(|| "注册失败: 无效响应".to_string())?;

    {
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = Some(auth.user.id.to_string());
    }
    {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = Some(auth.token.clone());
    }

    let c = config::Config {
        server_url,
        token: Some(auth.token.clone()),
        user_id: Some(auth.user.id.to_string()),
        user_email: Some(email),
    };
    config::save_config(&c).map_err(|e| format!("保存配置失败: {}", e))?;

    let _ = app.emit("auth-changed", true);

    Ok(AuthResult {
        token: auth.token,
        user_id: auth.user.id.to_string(),
        email: auth.user.email,
    })
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = None;
    }
    {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = None;
    }

    let c = config::Config {
        server_url: state.server_url.lock().map_err(|e| e.to_string())?.clone(),
        token: None,
        user_id: None,
        user_email: None,
    };
    config::save_config(&c).map_err(|e| format!("保存配置失败: {}", e))?;

    let _ = app.emit("auth-changed", false);

    Ok(())
}

#[tauri::command]
pub async fn get_server_url(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.server_url.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub async fn set_server_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
    {
        let mut server_url = state.server_url.lock().map_err(|e| e.to_string())?;
        *server_url = url;
    }

    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();
    let current_user_id = state.user_id.lock().map_err(|e| e.to_string())?.clone();
    let current_token = state.token.lock().map_err(|e| e.to_string())?.clone();
    let current_email = config::load_config().user_email;

    let c = config::Config {
        server_url,
        token: current_token,
        user_id: current_user_id,
        user_email: current_email,
    };
    config::save_config(&c).map_err(|e| format!("保存配置失败: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn get_current_user_id(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.user_id.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
pub async fn get_current_user_email(_state: State<'_, AppState>) -> Result<Option<String>, String> {
    let c = config::load_config();
    Ok(c.user_email)
}
