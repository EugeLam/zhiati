use std::sync::Mutex;
use serde_json;
use shared::{ApiResponse, CreateNoteRequest, UpdateNoteRequest, Note};
use tauri::Manager;

pub struct AppState {
    pub server_url: Mutex<String>,
    pub user_id: Mutex<Option<String>>,
    pub token: Mutex<Option<String>>,
}

fn get_token(state: &AppState) -> Result<String, String> {
    state.token.lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .clone()
        .ok_or_else(|| "未登录".to_string())
}

fn get_server_url(state: &AppState) -> Result<String, String> {
    state.server_url.lock()
        .map_err(|e| format!("Lock error: {}", e))
        .map(|s| s.clone())
}

fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

#[tauri::command]
pub async fn get_notes(state: tauri::State<'_, AppState>) -> Result<Vec<Note>, String> {
    tracing::info!("[Rust] get_notes called");

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/notes", server_url))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<Vec<Note>> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "获取便签失败".into()));
    }

    let body: ApiResponse<Vec<Note>> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let notes = body.data.ok_or_else(|| "无效响应".to_string())?;
    tracing::info!("[Rust] Returning {} notes", notes.len());
    Ok(notes)
}

#[tauri::command]
pub async fn create_note(
    state: tauri::State<'_, AppState>,
    title: String,
    content: Option<String>,
) -> Result<Note, String> {
    tracing::info!("[Rust] create_note called with title: {}", title);

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/notes", server_url))
        .header("Authorization", auth_header(&token))
        .json(&CreateNoteRequest {
            title,
            content,
            color: None,
        })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<Note> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "创建便签失败".into()));
    }

    let body: ApiResponse<Note> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let note = body.data.ok_or_else(|| "无效响应".to_string())?;
    tracing::info!("[Rust] Note created with id: {}", note.id);
    Ok(note)
}

#[tauri::command]
pub async fn update_note(
    state: tauri::State<'_, AppState>,
    id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<Note, String> {
    tracing::info!("[Rust] update_note called with id: {}", id);

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .put(format!("{}/api/notes/{}", server_url, id))
        .header("Authorization", auth_header(&token))
        .json(&UpdateNoteRequest {
            title,
            content,
            is_pinned: None,
            is_archived: None,
            color: None,
        })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<Note> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "更新便签失败".into()));
    }

    let body: ApiResponse<Note> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let note = body.data.ok_or_else(|| "无效响应".to_string())?;
    tracing::info!("[Rust] Note updated successfully");
    Ok(note)
}

#[tauri::command]
pub async fn delete_note(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    tracing::info!("[Rust] delete_note called with id: {}", id);

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .delete(format!("{}/api/notes/{}", server_url, id))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<()> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "删除便签失败".into()));
    }

    tracing::info!("[Rust] Note deleted successfully");
    Ok(())
}

#[tauri::command]
pub async fn sync_notes(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    tracing::info!("[Rust] sync_notes called");

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/notes", server_url))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        return Err("同步失败".to_string());
    }

    let body: ApiResponse<Vec<Note>> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let notes = body.data.ok_or_else(|| "无效响应".to_string())?;
    Ok(serde_json::json!({
        "success": true,
        "notes": notes
    }))
}

#[tauri::command]
pub async fn show_mini_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("mini") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn hide_mini_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("mini") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn toggle_always_on_top(app: tauri::AppHandle, window_label: String) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window(&window_label) {
        let is_on_top = window.is_always_on_top().unwrap_or(false);
        window.set_always_on_top(!is_on_top).map_err(|e| e.to_string())?;
        return Ok(!is_on_top);
    }
    Err("Window not found".to_string())
}

#[tauri::command]
pub async fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    tracing::info!("[Rust] show_main_window called");
    if let Some(window) = app.get_webview_window("main") {
        tracing::info!("[Rust] Found main window, showing and focusing");
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    } else {
        tracing::warn!("[Rust] Main window not found, recreating...");
        let url = tauri::WebviewUrl::External("http://localhost:5173".parse().unwrap());
        let _window = tauri::WebviewWindowBuilder::new(&app, "main", url)
            .title("纸条")
            .inner_size(900.0, 700.0)
            .min_inner_size(600.0, 400.0)
            .resizable(true)
            .center()
            .build()
            .map_err(|e| e.to_string())?;
        tracing::info!("[Rust] Main window recreated");
    }
    Ok(())
}

#[tauri::command]
pub async fn set_window_level(app: tauri::AppHandle, window_label: String, level: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&window_label) {
        match level.as_str() {
            "top" => {
                window.set_always_on_top(true).map_err(|e| e.to_string())?;
            }
            "bottom" => {
                window.hide().map_err(|e| e.to_string())?;
                window.show().map_err(|e| e.to_string())?;
            }
            "normal" => {
                window.set_always_on_top(false).map_err(|e| e.to_string())?;
            }
            _ => return Err("Invalid level".to_string()),
        }
    }
    Ok(())
}
