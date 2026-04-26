use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent},
};

pub fn setup_tray(
    app: &AppHandle,
    reminder_pending: Arc<AtomicBool>,
) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let icon = app.default_window_icon().cloned()
        .unwrap_or_else(|| tauri::image::Image::new(&[], 0, 0));

    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let mini_item = MenuItem::with_id(app, "mini", "显示迷你列表", true, None::<&str>)?;
    let hide_mini_item = MenuItem::with_id(app, "hide_mini", "隐藏迷你列表", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &mini_item, &hide_mini_item, &quit_item])?;

    let ack_pending = reminder_pending.clone();

    let tray = TrayIconBuilder::with_id("main")
        .icon(icon.clone())
        .menu(&menu)
        .tooltip("纸条 - 备忘录")
        .show_menu_on_left_click(false)
        .on_menu_event({
            let pending = ack_pending.clone();
            move |app, event| {
                tracing::info!("[Tray] Menu event: {}", event.id.as_ref());
                match event.id.as_ref() {
                    "show" => {
                        pending.store(false, Ordering::SeqCst);
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        } else {
                            tracing::warn!("[Tray] main window not found");
                        }
                    }
                    "mini" => {
                        pending.store(false, Ordering::SeqCst);
                        if let Some(window) = app.get_webview_window("mini") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        } else {
                            tracing::warn!("[Tray] mini window not found");
                        }
                    }
                    "hide_mini" => {
                        if let Some(window) = app.get_webview_window("mini") {
                            let _ = window.hide();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                }
            }
        })
        .on_tray_icon_event({
            let pending = ack_pending;
            move |tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    tracing::info!("[Tray] Left click");
                    let app = tray.app_handle();
                    pending.store(false, Ordering::SeqCst);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    } else {
                        tracing::warn!("[Tray] main window not found on click");
                    }
                }
            }
        })
        .build(app)?;

    // Background task: poll reminder_pending and flash tray icon when true
    let flash_app = app.clone();
    let pending = reminder_pending;
    tauri::async_runtime::spawn(async move {
        let icon = flash_app.default_window_icon().cloned()
            .unwrap_or_else(|| tauri::image::Image::new(&[], 0, 0));
        let mut flashing = false;
        let mut visible = true;
        loop {
            let is_pending = pending.load(Ordering::SeqCst);
            if is_pending && !flashing {
                tracing::info!("[Tray] Starting flash");
                flashing = true;
                visible = true;
            } else if !is_pending && flashing {
                tracing::info!("[Tray] Stopping flash, restoring icon");
                flashing = false;
                if let Some(tray) = flash_app.tray_by_id("main") {
                    let _ = tray.set_icon(Some(icon.clone()));
                }
            }
            if flashing {
                visible = !visible;
                if let Some(tray) = flash_app.tray_by_id("main") {
                    if visible {
                        let _ = tray.set_icon(Some(icon.clone()));
                    } else {
                        let _ = tray.set_icon(None);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    Ok(tray)
}
