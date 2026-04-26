//! Desktop reminder notification — a small borderless popup that slides out
//! from the bottom-right of the screen and auto-dismisses after 3 seconds.

use tauri::{AppHandle, Manager};
use std::io::Write;

pub fn show_notification(app: &AppHandle, title: &str, body: &str) {
    let title = html_escape(title);
    let body = html_escape(body);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tracing::info!("[Notification] show_notification: title={}, body={}", title, body);

        // If window already exists, just show it
        if let Some(window) = app.get_webview_window("reminder-notification") {
            tracing::info!("[Notification] Reusing existing window");
            let _ = window.show();
            let _ = window.set_focus();
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _ = window.hide();
            return;
        }

        // Determine position — bottom-right of primary monitor
        let x: i32;
        let y: i32;
        if let Ok(Some(monitor)) = app.primary_monitor() {
            let size = monitor.size();
            let pos = monitor.position();
            x = pos.x + (size.width as i32) - 360 - 24;
            y = pos.y + (size.height as i32) - 90 - 64;
            tracing::info!("[Notification] Position: x={}, y={}", x, y);
        } else {
            x = 1500;
            y = 900;
        }

        // Build HTML with injected title and body
        let html = format!(r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8"><style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:"Microsoft YaHei","Segoe UI",sans-serif;background:rgba(40,38,34,0.95);color:#F5EDDA;height:100vh;display:flex;align-items:center;padding:0 20px;border-radius:12px;border:1px solid rgba(232,181,71,0.35);box-shadow:0 8px 32px rgba(0,0,0,0.45);overflow:hidden}}
@keyframes slideIn{{from{{transform:translateX(100%);opacity:0}}to{{transform:translateX(0);opacity:1}}}}
@keyframes bellRing{{0%,100%{{transform:rotate(0)}}25%{{transform:rotate(15deg)}}75%{{transform:rotate(-15deg)}}}}
.container{{display:flex;align-items:center;gap:14px;animation:slideIn .35s cubic-bezier(.22,.61,.36,1);width:100%}}
.icon{{font-size:26px;animation:bellRing .5s ease-in-out 2;flex-shrink:0}}
.text{{flex:1;min-width:0}}
.t{{font-size:15px;font-weight:700;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
.b{{font-size:12px;opacity:.7;margin-top:2px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
</style></head><body>
<div class="container">
<span class="icon">&#128276;</span>
<div class="text">
<div class="t">{title}</div>
<div class="b">{body}</div>
</div>
</div>
</body></html>"#,
            title = title,
            body = body,
        );

        let temp_dir = std::env::temp_dir().join("zhiati");
        let _ = std::fs::create_dir_all(&temp_dir);
        let html_path = temp_dir.join("reminder-notification.html");

        if let Err(e) = std::fs::File::create(&html_path)
            .and_then(|mut f| f.write_all(html.as_bytes()))
        {
            tracing::error!("[Notification] Failed to write HTML: {}", e);
            return;
        }

        let file_url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));

        let url = match file_url.parse() {
            Ok(u) => tauri::WebviewUrl::External(u),
            Err(e) => {
                tracing::error!("[Notification] Failed to parse URL: {}", e);
                return;
            }
        };

        let window = match tauri::WebviewWindowBuilder::new(
            &app,
            "reminder-notification",
            url,
        )
            .title("提醒")
            .inner_size(360.0, 90.0)
            .position(x as f64, y as f64)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .transparent(true)
            .build()
        {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("[Notification] Failed to create window: {}", e);
                return;
            }
        };

        let _ = window.show();
        let _ = window.set_focus();
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let _ = window.hide();
    });
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
