#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;
mod commands;
mod auth;
mod config;
mod scheduler;
mod notification;

use commands::AppState;
use std::sync::{Mutex, Arc, atomic::AtomicBool};
use crate::scheduler::Scheduler;
use tauri::Manager;

fn is_another_instance_running() -> bool {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;

        let name: Vec<u16> = OsStr::new("Global\\zhiati-desktop-single-instance")
            .encode_wide()
            .chain(Some(0))
            .collect();

        unsafe {
            let handle = windows_sys::Win32::System::Threading::CreateMutexW(
                ptr::null(),
                1,
                name.as_ptr(),
            );

            if handle.is_null() {
                tracing::warn!("[Rust] CreateMutexW failed, allowing launch");
                return false;
            }

            if windows_sys::Win32::Foundation::GetLastError()
                == windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS
            {
                windows_sys::Win32::Foundation::CloseHandle(handle);
                return true;
            }

            let _leaked = handle;
            false
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .init();

    tracing::info!("[Rust] Application starting...");

    if is_another_instance_running() {
        tracing::warn!("[Rust] Another instance is already running, exiting");
        std::process::exit(0);
    }

    let lock_path = config::config_dir().join("app.lock");
    let _ = std::fs::remove_file(&lock_path);

    let cfg = config::load_config();
    tracing::info!("[Rust] Config loaded, server_url: {}", cfg.server_url);
    let _ = config::save_config(&cfg);

    let reminder_pending = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState {
            server_url: Mutex::new(cfg.server_url.clone()),
            user_id: Mutex::new(cfg.user_id),
            token: Mutex::new(cfg.token.clone()),
            scheduler: Scheduler::new(),
            reminder_pending: reminder_pending.clone(),
        })
        .setup(move |app| {
            tracing::info!("[Rust] App setup started");

            // Prevent main window from being destroyed on close — just hide it
            if let Some(window) = app.get_webview_window("main") {
                let cloned = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = cloned.hide();
                    }
                });
            }

            if let Err(e) = tray::setup_tray(app.handle(), reminder_pending.clone()) {
                tracing::error!("[Rust] Failed to setup tray: {}", e);
            }

            // Initialize reminder scheduler if user is logged in
            let state = app.state::<AppState>();
            let token = state.token.lock().unwrap().clone();
            let app_h = app.handle().clone();
            if token.is_some() {
                tauri::async_runtime::spawn(async move {
                    let state = app_h.state::<AppState>();
                    let t = state.token.lock().unwrap().clone().unwrap_or_default();
                    let url = state.server_url.lock().unwrap().clone();
                    state.scheduler.init(app_h.clone(), &url, &t).await;
                });
            }

            tracing::info!("[Rust] App setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_notes,
            commands::create_note,
            commands::update_note,
            commands::delete_note,
            commands::sync_notes,
            commands::get_reminders,
            commands::add_reminder,
            commands::delete_reminder,
            commands::show_mini_window,
            commands::hide_mini_window,
            commands::toggle_always_on_top,
            commands::set_window_level,
            commands::show_main_window,
            commands::test_reminder,
            auth::login,
            auth::register,
            auth::logout,
            auth::get_server_url,
            auth::set_server_url,
            auth::get_current_user_id,
            auth::get_current_user_email,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
