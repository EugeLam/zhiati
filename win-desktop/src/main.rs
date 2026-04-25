#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;
mod commands;
mod auth;
mod config;

use commands::AppState;
use std::sync::Mutex;

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

            // Raw pointer, no Drop — kernel handle stays alive until process exits
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

    // Single instance check (named mutex on Windows)
    if is_another_instance_running() {
        tracing::warn!("[Rust] Another instance is already running, exiting");
        std::process::exit(0);
    }

    // Clean up legacy PID lock file
    let lock_path = config::config_dir().join("app.lock");
    let _ = std::fs::remove_file(&lock_path);

    let cfg = config::load_config();
    tracing::info!("[Rust] Config loaded, server_url: {}", cfg.server_url);
    // Ensure config file exists so user can edit it
    let _ = config::save_config(&cfg);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState {
            server_url: Mutex::new(cfg.server_url),
            user_id: Mutex::new(cfg.user_id),
            token: Mutex::new(cfg.token),
        })
        .setup(|app| {
            tracing::info!("[Rust] App setup started");
            if let Err(e) = tray::setup_tray(app.handle()) {
                tracing::error!("[Rust] Failed to setup tray: {}", e);
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
            commands::show_mini_window,
            commands::hide_mini_window,
            commands::toggle_always_on_top,
            commands::set_window_level,
            commands::show_main_window,
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
