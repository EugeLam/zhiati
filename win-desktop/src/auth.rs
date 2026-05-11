use crate::commands::AppState;
use crate::config;
use serde::Serialize;
use shared::{ApiResponse, AuthResponse, LoginRequest, RegisterRequest};
use tauri::{AppHandle, Emitter, State};

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .proxy(reqwest::Proxy::custom(|_| None::<String>))
        .no_proxy()
        .build()
        .expect("Failed to build reqwest client")
}

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

    let client = build_client();
    let resp = client
        .post(format!("{}/api/auth/login", server_url))
        .json(&LoginRequest { email, password })
        .send()
        .await
        .map_err(|e| {
            format!("无法连接到服务器: {}", e)
        })?;

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

    save_auth_state(&state, &app, &auth, server_url)
}

#[tauri::command]
pub async fn register(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<AuthResult, String> {
    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();

    let client = build_client();
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

    save_auth_state(&state, &app, &auth, server_url)
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Clear cloud token only, keep local credentials and data
    {
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = None;
    }
    {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = None;
    }

    let cfg = config::load_config();
    let c = config::Config {
        server_url: cfg.server_url,
        token: None,
        user_id: None,
        user_email: None,
        local_email: cfg.local_email,
        local_password_encrypted: cfg.local_password_encrypted,
        bound_cloud_email: cfg.bound_cloud_email,
        cloud_enabled: cfg.cloud_enabled,
        attachments_root: cfg.attachments_root,
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

    let cfg = config::load_config();
    let c = config::Config {
        server_url: state.server_url.lock().map_err(|e| e.to_string())?.clone(),
        token: state.token.lock().map_err(|e| e.to_string())?.clone(),
        user_id: state.user_id.lock().map_err(|e| e.to_string())?.clone(),
        user_email: cfg.user_email,
        local_email: cfg.local_email,
        local_password_encrypted: cfg.local_password_encrypted,
        bound_cloud_email: cfg.bound_cloud_email,
        cloud_enabled: cfg.cloud_enabled,
        attachments_root: cfg.attachments_root,
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
    Ok(c.local_email.or(c.user_email))
}

/// Transparent login: try login first, if 401 then register then login
pub async fn transparent_cloud_login(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<AuthResult, String> {
    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();
    let client = build_client();

    // Try login first
    let resp = client
        .post(format!("{}/api/auth/login", server_url))
        .json(&LoginRequest { email: email.clone(), password: password.clone() })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if resp.status().is_success() {
        let body: ApiResponse<AuthResponse> = resp
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;
        if let Some(auth) = body.data {
            return save_auth_state(&state, &app, &auth, server_url);
        }
        return Err("登录失败: 无效响应".to_string());
    }

    // If 401 or other error, try transparent registration
    let resp = client
        .post(format!("{}/api/auth/register", server_url))
        .json(&RegisterRequest { email: email.clone(), password: password.clone() })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<AuthResponse> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "云端账号注册失败".into()));
    }

    // Login after successful registration
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
        return Err(body.error.unwrap_or_else(|| "注册后登录失败".into()));
    }

    let body: ApiResponse<AuthResponse> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let auth = body.data.ok_or_else(|| "注册后登录失败: 无效响应".to_string())?;

    save_auth_state(&state, &app, &auth, server_url)
}

fn save_auth_state(
    state: &State<'_, AppState>,
    app: &AppHandle,
    auth: &AuthResponse,
    server_url: String,
) -> Result<AuthResult, String> {
    {
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = Some(auth.user.id.to_string());
    }
    {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = Some(auth.token.clone());
    }

    let cfg = config::load_config();
    let c = config::Config {
        server_url: server_url.clone(),
        token: Some(auth.token.clone()),
        user_id: Some(auth.user.id.to_string()),
        user_email: Some(auth.user.email.clone()),
        local_email: cfg.local_email,
        local_password_encrypted: cfg.local_password_encrypted,
        bound_cloud_email: cfg.bound_cloud_email,
        cloud_enabled: cfg.cloud_enabled,
        attachments_root: cfg.attachments_root,
    };
    config::save_config(&c).map_err(|e| format!("保存配置失败: {}", e))?;

    let _ = app.emit("auth-changed", true);

    Ok(AuthResult {
        token: auth.token.clone(),
        user_id: auth.user.id.to_string(),
        email: auth.user.email.clone(),
    })
}

/// Bind an existing cloud account to the local account
#[tauri::command]
pub async fn bind_cloud_account(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<AuthResult, String> {
    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();

    let client = build_client();
    let resp = client
        .post(format!("{}/api/auth/login", server_url))
        .json(&LoginRequest { email: email.clone(), password: password.clone() })
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

    // Save auth state with binding
    {
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = Some(auth.user.id.to_string());
    }
    {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = Some(auth.token.clone());
    }

    let cfg = config::load_config();
    let c = config::Config {
        server_url: server_url.clone(),
        token: Some(auth.token.clone()),
        user_id: Some(auth.user.id.to_string()),
        user_email: Some(auth.user.email.clone()),
        local_email: cfg.local_email,
        local_password_encrypted: Some(crate::crypto::encrypt_password(&password)),
        bound_cloud_email: Some(email.clone()),
        cloud_enabled: cfg.cloud_enabled,
        attachments_root: cfg.attachments_root,
    };
    config::save_config(&c).map_err(|e| format!("保存配置失败: {}", e))?;

    let _ = app.emit("auth-changed", true);

    Ok(AuthResult {
        token: auth.token.clone(),
        user_id: auth.user.id.to_string(),
        email: auth.user.email.clone(),
    })
}

/// Register a new cloud account and bind it to the local account
#[tauri::command]
pub async fn register_and_bind(
    app: AppHandle,
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<AuthResult, String> {
    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();

    let client = build_client();

    // Register first
    let resp = client
        .post(format!("{}/api/auth/register", server_url))
        .json(&RegisterRequest { email: email.clone(), password: password.clone() })
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

    // Login after registration
    let resp = client
        .post(format!("{}/api/auth/login", server_url))
        .json(&LoginRequest { email: email.clone(), password: password.clone() })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<AuthResponse> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "注册后登录失败".into()));
    }

    let body: ApiResponse<AuthResponse> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let auth = body.data.ok_or_else(|| "注册后登录失败: 无效响应".to_string())?;

    // Save auth state with binding
    {
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = Some(auth.user.id.to_string());
    }
    {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = Some(auth.token.clone());
    }

    let cfg = config::load_config();
    let c = config::Config {
        server_url: server_url.clone(),
        token: Some(auth.token.clone()),
        user_id: Some(auth.user.id.to_string()),
        user_email: Some(auth.user.email.clone()),
        local_email: cfg.local_email,
        local_password_encrypted: Some(crate::crypto::encrypt_password(&password)),
        bound_cloud_email: Some(email),
        cloud_enabled: cfg.cloud_enabled,
        attachments_root: cfg.attachments_root,
    };
    config::save_config(&c).map_err(|e| format!("保存配置失败: {}", e))?;

    let _ = app.emit("auth-changed", true);

    Ok(AuthResult {
        token: auth.token.clone(),
        user_id: auth.user.id.to_string(),
        email: auth.user.email.clone(),
    })
}
