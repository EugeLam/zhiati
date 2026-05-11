use std::sync::{Mutex, Arc, atomic::{AtomicBool, Ordering}};
use shared::{ApiResponse, CreateNoteRequest, UpdateNoteRequest, Note, Reminder, CreateReminderRequest};
use sqlx::SqlitePool;
use tauri::Manager;
use crate::scheduler::Scheduler;

pub struct AppState {
    pub server_url: Mutex<String>,
    pub user_id: Mutex<Option<String>>,
    pub token: Mutex<Option<String>>,
    pub scheduler: Scheduler,
    pub reminder_pending: Arc<AtomicBool>,
    pub db: SqlitePool,
    pub cloud_enabled: Mutex<bool>,
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

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .proxy(reqwest::Proxy::custom(|_| None::<String>))
        .no_proxy()
        .build()
        .expect("Failed to build reqwest client")
}

#[tauri::command]
pub async fn get_notes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Note>, String> {
    let notes = crate::db::get_notes(&state.db).await?;
    tracing::info!("[Rust] Returning {} notes from local DB", notes.len());
    Ok(notes)
}

#[tauri::command]
pub async fn create_note(
    state: tauri::State<'_, AppState>,
    title: String,
    content: String,
    color: Option<String>,
) -> Result<Note, String> {
    let title = if title.trim().is_empty() { "未命名便签" } else { &title };
    let user_id_str = state.user_id.lock().map_err(|e| e.to_string())?.clone().unwrap_or_default();
    let user_id = uuid::Uuid::parse_str(&user_id_str).unwrap_or(uuid::Uuid::nil());

    let now = chrono::Utc::now();
    let note = shared::Note {
        id: uuid::Uuid::new_v4(),
        user_id,
        title: title.to_string(),
        content: Some(content),
        is_pinned: false,
        is_archived: false,
        color: color.unwrap_or_else(|| "#FFFB00".to_string()),
        created_at: now,
        updated_at: now,
        synced_at: None,
    };

    // Always write to local DB first
    crate::db::create_note(&state.db, &note).await?;

    // Optionally push to cloud
    let cloud_on = *state.cloud_enabled.lock().map_err(|e| e.to_string())?;
    if cloud_on {
        if let (Ok(server_url), Ok(token)) = (get_server_url(&state), get_token(&state)) {
            let client = build_client();
            let _ = client
                .post(format!("{}/api/notes", server_url))
                .header("Authorization", auth_header(&token))
                .json(&CreateNoteRequest {
                    title: note.title.clone(),
                    content: note.content.clone(),
                    color: Some(note.color.clone()),
                })
                .send()
                .await;
        }
    }

    Ok(note)
}

#[tauri::command]
pub async fn update_note(
    state: tauri::State<'_, AppState>,
    id: String,
    title: String,
    content: String,
    color: Option<String>,
) -> Result<Note, String> {
    // Read existing note from local DB
    let existing = crate::db::get_note_by_id(&state.db, &id).await?;
    let mut note = existing.ok_or_else(|| format!("便签 {} 不存在", id))?;

    note.title = title;
    note.content = Some(content);
    if let Some(c) = color {
        note.color = c;
    }
    note.updated_at = chrono::Utc::now();

    // Update local DB
    crate::db::update_note(&state.db, &note).await?;

    // Optionally push to cloud
    let cloud_on = *state.cloud_enabled.lock().map_err(|e| e.to_string())?;
    if cloud_on {
        if let (Ok(server_url), Ok(token)) = (get_server_url(&state), get_token(&state)) {
            let client = build_client();
            let _ = client
                .put(format!("{}/api/notes/{}", server_url, id))
                .header("Authorization", auth_header(&token))
                .json(&UpdateNoteRequest {
                    title: Some(note.title.clone()),
                    content: note.content.clone(),
                    is_pinned: None,
                    is_archived: None,
                    color: Some(note.color.clone()),
                })
                .send()
                .await;
        }
    }

    Ok(note)
}

#[tauri::command]
pub async fn delete_note(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    // Delete from local DB (also deletes related reminders and note_tags)
    crate::db::delete_note(&state.db, &id).await?;

    // Optionally delete from cloud
    let cloud_on = *state.cloud_enabled.lock().map_err(|e| e.to_string())?;
    if cloud_on {
        if let (Ok(server_url), Ok(token)) = (get_server_url(&state), get_token(&state)) {
            let client = build_client();
            let _ = client
                .delete(format!("{}/api/notes/{}", server_url, id))
                .header("Authorization", auth_header(&token))
                .send()
                .await;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn sync_notes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Note>, String> {
    let cloud_on = *state.cloud_enabled.lock().map_err(|e| e.to_string())?;

    if cloud_on {
        // Cloud mode: push local notes to server, pull server state back
        let token = match get_token(&state) {
            Ok(t) => t,
            Err(_) => {
                // No cloud token, just reload from local
                return crate::db::get_notes(&state.db).await;
            }
        };
        let server_url = get_server_url(&state)?;

        // Push local notes to server
        let local_notes = crate::db::get_notes(&state.db).await?;
        let last_synced = crate::db::get_last_synced_at(&state.db).await?;

        let client = build_client();
        let sync_req = shared::SyncRequest {
            notes: local_notes,
            last_synced_at: last_synced,
        };
        let resp = client
            .post(format!("{}/api/notes/sync", server_url))
            .header("Authorization", auth_header(&token))
            .json(&sync_req)
            .send()
            .await
            .map_err(|e| format!("同步失败: 无法连接到服务器: {}", e))?;

        if resp.status().is_success() {
            let body: ApiResponse<shared::SyncResponse> = resp
                .json()
                .await
                .map_err(|e| format!("解析同步响应失败: {}", e))?;
            if let Some(sync_resp) = body.data {
                // Upsert all server notes into local DB
                crate::db::upsert_all_notes(&state.db, &sync_resp.notes).await?;
                crate::db::set_last_synced_at(&state.db, sync_resp.synced_at).await?;
                return Ok(sync_resp.notes);
            }
        }

        // If sync failed, just return local notes
        crate::db::get_notes(&state.db).await
    } else {
        // Local-only mode: just reload from local DB (refresh)
        crate::db::get_notes(&state.db).await
    }
}

#[tauri::command]
pub async fn get_reminders(
    state: tauri::State<'_, AppState>,
    note_id: String,
) -> Result<Vec<Reminder>, String> {
    tracing::info!("[Rust] get_reminders called for note: {}", note_id);
    crate::db::get_reminders(&state.db, Some(&note_id)).await
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

    let remind_dt: chrono::DateTime<chrono::Utc> = remind_at
        .parse()
        .map_err(|e| format!("无效的时间格式: {}", e))?;

    let note_uuid = uuid::Uuid::parse_str(&note_id)
        .map_err(|e| format!("无效的便签ID: {}", e))?;

    let user_id_str = state.user_id.lock().map_err(|e| e.to_string())?.clone().unwrap_or_default();
    let user_id = uuid::Uuid::parse_str(&user_id_str).unwrap_or(uuid::Uuid::nil());
    let now = chrono::Utc::now();

    let reminder = Reminder {
        id: uuid::Uuid::new_v4(),
        note_id: note_uuid,
        user_id,
        remind_at: remind_dt,
        is_triggered: false,
        created_at: now,
        updated_at: now,
        note_title: Some(note_title),
        note_content: Some(note_content),
    };

    // Save to local DB
    crate::db::create_reminder(&state.db, &reminder).await?;

    // Schedule local timer
    let server_url = state.server_url.lock().map_err(|e| e.to_string())?.clone();
    let token = state.token.lock().map_err(|e| e.to_string())?.clone().unwrap_or_default();
    state.scheduler.schedule(
        app.clone(),
        reminder.clone(),
        server_url.clone(),
        token.clone(),
        reminder.note_title.clone().unwrap_or_default(),
        reminder.note_content.clone().unwrap_or_default(),
    );

    // Optionally push to cloud
    let cloud_on = *state.cloud_enabled.lock().map_err(|e| e.to_string())?;
    if cloud_on && !token.is_empty() {
        let client = build_client();
        let _ = client
            .post(format!("{}/api/reminders", server_url))
            .header("Authorization", auth_header(&token))
            .json(&CreateReminderRequest {
                note_id: note_uuid,
                remind_at: remind_dt,
            })
            .send()
            .await;
    }

    tracing::info!("[Rust] Reminder created with id: {}", reminder.id);
    Ok(reminder)
}

#[tauri::command]
pub async fn delete_reminder(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    tracing::info!("[Rust] delete_reminder called with id: {}", id);

    // Cancel local timer
    state.scheduler.cancel(&id);

    // Delete from local DB
    crate::db::delete_reminder(&state.db, &id).await?;

    // Optionally delete from cloud
    let cloud_on = *state.cloud_enabled.lock().map_err(|e| e.to_string())?;
    if cloud_on {
        if let (Ok(server_url), Ok(token)) = (get_server_url(&state), get_token(&state)) {
            let client = build_client();
            let _ = client
                .delete(format!("{}/api/reminders/{}", server_url, id))
                .header("Authorization", auth_header(&token))
                .send()
                .await;
        }
    }

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

#[derive(serde::Serialize)]
pub struct ImageUploadResult {
    pub filename: String,
    pub url: String,
    pub local_path: String,
}

#[tauri::command]
pub async fn upload_image(
    state: tauri::State<'_, AppState>,
    file_path: String,
    note_id: String,
) -> Result<ImageUploadResult, String> {
    tracing::info!("[Rust] upload_image called: {}", file_path);

    let file_bytes = tokio::fs::read(&file_path)
        .await
        .map_err(|e| format!("读取文件失败: {}", e))?;
    tracing::info!("[Rust] File size: {} bytes", file_bytes.len());

    let ext = std::path::Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or("png".to_string());
    let file_name = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image.png")
        .to_string();
    let mime_type = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };

    // 1. Save local copy to attachments/{note_id}/
    let root = crate::config::ensure_attachments_root();
    let note_dir = root.join("attachments").join(&note_id);
    tokio::fs::create_dir_all(&note_dir)
        .await
        .map_err(|e| format!("创建附件目录失败: {}", e))?;
    let local_file_name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let local_path = note_dir.join(&local_file_name);
    tokio::fs::write(&local_path, &file_bytes)
        .await
        .map_err(|e| format!("保存本地副本失败: {}", e))?;

    // Store relative path from attachments_root: attachments/{note_id}/{uuid}.ext
    let relative_path = format!("attachments/{}/{}", note_id, local_file_name);
    tracing::info!("[Rust] Saved local copy: {}", relative_path);

    // 2. Try upload to S3 (non-fatal if unavailable)
    let server_url = get_server_url(&state).ok();
    let token = get_token(&state).ok();
    let mut s3_url: Option<String> = None;

    if let (Some(srv), Some(tok)) = (&server_url, &token) {
        let part = reqwest::multipart::Part::bytes(file_bytes.clone())
            .file_name(file_name.clone())
            .mime_str(mime_type)
            .map_err(|e| format!("设置文件类型失败: {}", e))?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("note_id", note_id.clone());
        let upload_url = format!("{}/api/attachments/upload", srv);
        let client = build_client();

        match client
            .post(&upload_url)
            .header("Authorization", auth_header(tok))
            .multipart(form)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp
                    .json::<shared::ApiResponse<shared::AttachmentUploadResponse>>()
                    .await
                {
                    if let Some(data) = body.data {
                        s3_url = Some(data.url.clone());
                        tracing::info!("[Rust] Uploaded to S3: {}", data.url);
                    }
                }
            }
            Ok(resp) => {
                let err = resp.text().await.unwrap_or_default();
                tracing::warn!("[Rust] S3 upload failed: {}", err);
            }
            Err(e) => {
                tracing::warn!("[Rust] S3 upload error: {:?}", e);
            }
        }
    }

    // Return S3 URL if available, otherwise relative path (frontend resolves to actual location)
    let final_url = s3_url.unwrap_or_else(|| relative_path.clone());

    Ok(ImageUploadResult {
        filename: file_name,
        url: final_url,
        local_path: relative_path,
    })
}

/// Download an attachment from cloud (S3 URL) to local cache, return relative path.
/// If already cached, return the cached relative path directly.
#[tauri::command]
pub async fn download_attachment(
    _state: tauri::State<'_, AppState>,
    url: String,
    note_id: String,
    filename: String,
) -> Result<String, String> {
    tracing::info!("[Rust] download_attachment: url={} note_id={}", url, note_id);

    let root = crate::config::ensure_attachments_root();
    let note_dir = root.join("attachments").join(&note_id);
    tokio::fs::create_dir_all(&note_dir)
        .await
        .map_err(|e| format!("创建附件目录失败: {}", e))?;

    // Check if already cached
    let local_path = note_dir.join(&filename);
    if local_path.exists() {
        let relative = format!("attachments/{}/{}", note_id, filename);
        tracing::info!("[Rust] Cache hit: {}", relative);
        return Ok(relative);
    }

    // Download from S3/cloud URL
    let client = build_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    tokio::fs::write(&local_path, &bytes)
        .await
        .map_err(|e| format!("保存文件失败: {}", e))?;

    let relative = format!("attachments/{}/{}", note_id, filename);
    tracing::info!("[Rust] Downloaded: {} ({} bytes)", relative, bytes.len());
    Ok(relative)
}

/// Resolve a relative attachment path (attachments/note_id/filename) to absolute path.
/// The frontend uses this to convert to asset:// URLs via convertFileSrc.
#[tauri::command]
pub async fn resolve_attachment_path(path: String) -> Result<String, String> {
    let root = crate::config::attachments_root();
    let full_path = root.join(&path);
    if !full_path.exists() {
        return Err(format!("附件不存在: {}", path));
    }
    Ok(full_path.to_string_lossy().replace('\\', "/"))
}

/// Read an attachment file and return its content as a base64 data URL.
/// This is a fallback for when the asset protocol is blocked by scope restrictions.
#[tauri::command]
pub async fn read_attachment_as_data_url(path: String) -> Result<String, String> {
    let root = crate::config::attachments_root();
    let full_path = root.join(&path);
    if !full_path.exists() {
        return Err(format!("附件不存在: {}", path));
    }
    let bytes = tokio::fs::read(&full_path)
        .await
        .map_err(|e| format!("读取文件失败: {}", e))?;
    let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let mime = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, encoded))
}

/// Get the current attachments root directory path.
#[tauri::command]
pub async fn get_attachments_root() -> Result<String, String> {
    let root = crate::config::attachments_root();
    Ok(root.to_string_lossy().replace('\\', "/"))
}

/// Change attachments root directory and migrate existing files.
#[tauri::command]
pub async fn set_attachments_root(new_root: String) -> Result<(), String> {
    let old_root = crate::config::attachments_root();
    let new_root_path = std::path::PathBuf::from(&new_root);

    // Validate: new root must be writable
    std::fs::create_dir_all(&new_root_path)
        .map_err(|e| format!("无法创建目录: {}", e))?;

    // Migrate existing files if old root differs
    if old_root != new_root_path && old_root.exists() {
        tracing::info!("[Rust] Migrating attachments from {:?} to {:?}", old_root, new_root_path);
        crate::config::migrate_attachments(old_root, new_root_path.clone())?;
    }

    // Save config
    let mut cfg = crate::config::load_config();
    cfg.attachments_root = Some(new_root.clone());
    crate::config::save_config(&cfg)?;

    tracing::info!("[Rust] Attachments root set to: {}", new_root);
    Ok(())
}

/// Get storage statistics for attachments directory.
#[tauri::command]
pub async fn get_attachments_storage_info(_root: String) -> Result<StorageInfo, String> {
    let root = crate::config::attachments_root();
    let mut total_size: u64 = 0;
    let mut file_count: u64 = 0;

    if root.exists() {
        for entry in walkdir::WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
                file_count += 1;
            }
        }
    }

    Ok(StorageInfo { total_size, file_count })
}

#[derive(serde::Serialize)]
pub struct StorageInfo {
    pub total_size: u64,
    pub file_count: u64,
}

#[derive(serde::Serialize)]
pub struct AppMode {
    pub cloud_enabled: bool,
    pub is_cloud_connected: bool,
    pub local_account_exists: bool,
    pub cloud_account_bound: bool,
}

#[tauri::command]
pub async fn get_app_mode(state: tauri::State<'_, AppState>) -> Result<AppMode, String> {
    let cloud_on = *state.cloud_enabled.lock().map_err(|e| e.to_string())?;
    let cloud_connected = state.token.lock().map_err(|e| e.to_string())?.is_some();
    let cfg = crate::config::load_config();
    let local_account_exists = cfg.local_email.is_some() && cfg.local_password_encrypted.is_some();
    let cloud_account_bound = cfg.bound_cloud_email.is_some();
    Ok(AppMode {
        cloud_enabled: cloud_on,
        is_cloud_connected: cloud_connected,
        local_account_exists,
        cloud_account_bound,
    })
}

#[tauri::command]
pub async fn setup_local_account(
    email: String,
    password: String,
) -> Result<(), String> {
    let mut cfg = crate::config::load_config();
    cfg.local_email = Some(email);
    cfg.local_password_encrypted = Some(crate::crypto::encrypt_password(&password));
    crate::config::save_config(&cfg)
}

#[tauri::command]
pub async fn toggle_cloud(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    {
        let mut cloud = state.cloud_enabled.lock().map_err(|e| e.to_string())?;
        *cloud = enabled;
    }
    let mut cfg = crate::config::load_config();
    cfg.cloud_enabled = enabled;
    if !enabled {
        // When disabling cloud, clear cloud token but keep local credentials
        cfg.token = None;
        cfg.user_id = None;
    }
    crate::config::save_config(&cfg)?;

    // Also update state's token/user_id
    if !enabled {
        let mut token = state.token.lock().map_err(|e| e.to_string())?;
        *token = None;
        let mut user_id = state.user_id.lock().map_err(|e| e.to_string())?;
        *user_id = None;
    }

    Ok(())
}
