use std::sync::{Mutex, Arc, atomic::{AtomicBool, Ordering}};
use shared::{ApiResponse, CreateNoteRequest, UpdateNoteRequest, Note, Reminder, CreateReminderRequest};
use tauri::Manager;
use crate::scheduler::Scheduler;

pub struct AppState {
    pub server_url: Mutex<String>,
    pub user_id: Mutex<Option<String>>,
    pub token: Mutex<Option<String>>,
    pub scheduler: Scheduler,
    pub reminder_pending: Arc<AtomicBool>,
}

fn get_token(state: &AppState) -> Result<String, String> {
    state.token.lock()
        .map_err(|e| format!("获取令牌失败: {}", e))?
        .clone()
        .ok_or_else(|| "未登录".to_string())
}

fn get_server_url(state: &AppState) -> Result<String, String> {
    Ok(state.server_url.lock()
        .map_err(|e| format!("获取服务器URL失败: {}", e))?
        .clone())
}

fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

#[tauri::command]
pub async fn get_notes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Note>, String> {
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
        return Err("服务器返回错误".to_string());
    }

    let body: ApiResponse<Vec<Note>> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let notes = body.data.unwrap_or_default();
    tracing::info!("[Rust] Returning {} notes", notes.len());
    Ok(notes)
}

#[tauri::command]
pub async fn create_note(
    state: tauri::State<'_, AppState>,
    title: String,
    content: String,
    color: String,
) -> Result<Note, String> {
    let title = if title.trim().is_empty() { "未命名便签" } else { &title };
    let color_opt = if color.is_empty() { None } else { Some(color) };

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/notes", server_url))
        .header("Authorization", auth_header(&token))
        .json(&CreateNoteRequest {
            title: title.to_string(),
            content: Some(content),
            color: color_opt,
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

    body.data.ok_or_else(|| "无效响应".to_string())
}

#[tauri::command]
pub async fn update_note(
    state: tauri::State<'_, AppState>,
    id: String,
    title: String,
    content: String,
    color: String,
) -> Result<Note, String> {
    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .put(format!("{}/api/notes/{}", server_url, id))
        .header("Authorization", auth_header(&token))
        .json(&UpdateNoteRequest {
            title: Some(title),
            content: Some(content),
            is_pinned: None,
            is_archived: None,
            color: Some(color),
        })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        return Err("服务器返回错误".to_string());
    }

    let body: ApiResponse<Note> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    body.data.ok_or_else(|| "无效响应".to_string())
}

#[tauri::command]
pub async fn delete_note(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
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
        return Err("删除失败".to_string());
    }

    Ok(())
}

#[tauri::command]
pub async fn sync_notes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Note>, String> {
    get_notes(state).await
}

#[tauri::command]
pub async fn get_reminders(
    state: tauri::State<'_, AppState>,
    note_id: String,
) -> Result<Vec<Reminder>, String> {
    tracing::info!("[Rust] get_reminders called for note: {}", note_id);

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/reminders?note_id={}", server_url, note_id))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        return Ok(vec![]);
    }

    let body: ApiResponse<Vec<Reminder>> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    Ok(body.data.unwrap_or_default())
}

#[tauri::command]
pub async fn add_reminder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    note_id: String,
    remind_at: String,
    note_title: String,
    note_content: String,
) -> Result<Reminder, String> {
    tracing::info!("[Rust] add_reminder called for note: {}", note_id);

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let remind_dt: chrono::DateTime<chrono::Utc> = remind_at
        .parse()
        .map_err(|e| format!("无效的时间格式: {}", e))?;

    let note_uuid = uuid::Uuid::parse_str(&note_id)
        .map_err(|e| format!("无效的便签ID: {}", e))?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/reminders", server_url))
        .header("Authorization", auth_header(&token))
        .json(&CreateReminderRequest {
            note_id: note_uuid,
            remind_at: remind_dt,
        })
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<Reminder> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "创建提醒失败".into()));
    }

    let body: ApiResponse<Reminder> = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let reminder = body.data.ok_or_else(|| "无效响应".to_string())?;

    // Schedule local timer
    state.scheduler.schedule(
        app.clone(),
        reminder.clone(),
        server_url,
        token,
        note_title,
        note_content,
    );

    tracing::info!("[Rust] Reminder created with id: {}", reminder.id);
    Ok(reminder)
}

#[tauri::command]
pub async fn delete_reminder(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    tracing::info!("[Rust] delete_reminder called with id: {}", id);

    let server_url = get_server_url(&state)?;
    let token = get_token(&state)?;

    let client = reqwest::Client::new();
    let resp = client
        .delete(format!("{}/api/reminders/{}", server_url, id))
        .header("Authorization", auth_header(&token))
        .send()
        .await
        .map_err(|e| format!("无法连接到服务器: {}", e))?;

    if !resp.status().is_success() {
        let body: ApiResponse<()> = resp
            .json()
            .await
            .unwrap_or_else(|_| ApiResponse::error("Unknown error".into()));
        return Err(body.error.unwrap_or_else(|| "删除提醒失败".into()));
    }

    state.scheduler.cancel(&id);
    Ok(())
}

#[tauri::command]
pub async fn show_mini_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Stop tray flashing
    state.reminder_pending.store(false, Ordering::SeqCst);
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
pub async fn test_reminder(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("[Rust] test_reminder called - triggering reminder directly");
    state.reminder_pending.store(true, Ordering::SeqCst);

    // Show desktop notification window
    crate::notification::show_notification(&app, "测试提醒", "这是一条测试提醒消息");

    // Also send native notification
    use tauri_plugin_notification::NotificationExt;
    let result = app
        .notification()
        .builder()
        .title("测试提醒")
        .body("这是一条测试提醒消息")
        .show();
    tracing::info!("[Rust] test_reminder notification result: {:?}", result);
    Ok(result.map_err(|e| e.to_string())?)
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
pub async fn show_main_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("[Rust] show_main_window called");
    // Stop tray flashing
    state.reminder_pending.store(false, Ordering::SeqCst);
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
            .fullscreen(false)
            .decorations(true)
            .transparent(false)
            .center()
            .build()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_window_level(app: tauri::AppHandle, window_label: String, level: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&window_label) {
        match level.as_str() {
            "normal" => window.set_always_on_top(false).map_err(|e| e.to_string())?,
            "always_on_top" => window.set_always_on_top(true).map_err(|e| e.to_string())?,
            _ => {}
        }
        return Ok(());
    }
    Err("Window not found".to_string())
}
