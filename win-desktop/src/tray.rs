use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::{TrayIcon, TrayIconBuilder, MouseButton, MouseButtonState, TrayIconEvent},
};

pub fn setup_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let mini_item = MenuItem::with_id(app, "mini", "显示迷你列表", true, None::<&str>)?;
    let hide_mini_item = MenuItem::with_id(app, "hide_mini", "隐藏迷你列表", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &mini_item, &hide_mini_item, &quit_item])?;

    let icon = app.default_window_icon()
        .cloned()
        .unwrap_or_else(|| tauri::image::Image::new(&[], 0, 0));

    let tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("纸条 - 备忘录")
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    tracing::info!("Tray: show main window clicked");
                    if let Some(window) = app.get_webview_window("main") {
                        tracing::info!("Showing main window");
                        let _ = window.show();
                        let _ = window.set_focus();
                    } else {
                        tracing::warn!("Main window not found!");
                    }
                }
                "mini" => {
                    tracing::info!("Tray: mini window clicked");
                    if let Some(window) = app.get_webview_window("mini") {
                        tracing::info!("Showing mini window");
                        let _ = window.show();
                        let _ = window.set_focus();
                    } else {
                        tracing::warn!("Mini window not found!");
                    }
                }
                "hide_mini" => {
                    tracing::info!("Tray: hide mini clicked");
                    if let Some(window) = app.get_webview_window("mini") {
                        let _ = window.hide();
                    }
                }
                "quit" => {
                    tracing::info!("Tray: quit clicked");
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                tracing::info!("Tray icon left clicked");
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    tracing::info!("Showing main window from tray click");
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(tray)
}
