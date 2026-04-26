use std::collections::HashMap;
use std::sync::{Mutex, atomic::Ordering};
use tokio::task::JoinHandle;
use shared::Reminder;
use tauri::Manager;

pub struct Scheduler {
    timers: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            timers: Mutex::new(HashMap::new()),
        }
    }

    /// Initialize scheduler by fetching all pending reminders and scheduling them.
    pub async fn init(
        &self,
        app: tauri::AppHandle,
        server_url: &str,
        token: &str,
    ) {
        tracing::info!("[Scheduler] Initializing, fetching pending reminders...");

        let client = reqwest::Client::new();
        let resp = match client
            .get(format!("{}/api/reminders", server_url))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("[Scheduler] Failed to fetch reminders: {}", e);
                return;
            }
        };

        if !resp.status().is_success() {
            tracing::warn!("[Scheduler] Server returned non-success status");
            return;
        }

        let body: shared::ApiResponse<Vec<Reminder>> = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("[Scheduler] Failed to parse reminders: {}", e);
                return;
            }
        };

        let reminders = body.data.unwrap_or_default();
        tracing::info!("[Scheduler] Loaded {} pending reminders", reminders.len());

        for reminder in reminders {
            let note_title = reminder.note_title.clone().unwrap_or_else(|| "便签提醒".to_string());
            let note_body = reminder.note_content.clone().unwrap_or_else(|| "您有一条便签提醒到了".to_string());
            self.schedule(
                app.clone(),
                reminder,
                server_url.to_string(),
                token.to_string(),
                note_title,
                note_body,
            );
        }
    }

    /// Schedule a local timer for a reminder.
    pub fn schedule(
        &self,
        app: tauri::AppHandle,
        reminder: Reminder,
        server_url: String,
        token: String,
        note_title: String,
        note_body: String,
    ) {
        let reminder_id = reminder.id.to_string();
        let remind_at = reminder.remind_at;

        let now = chrono::Utc::now();
        let delay = if remind_at > now {
            let d = (remind_at - now).to_std().unwrap_or(std::time::Duration::from_secs(1));
            if d < std::time::Duration::from_secs(1) {
                std::time::Duration::from_secs(1)
            } else {
                d
            }
        } else {
            // Past reminder — trigger immediately with small delay
            std::time::Duration::from_secs(2)
        };

        let timer_id = reminder_id.clone();
        tracing::info!(
            "[Scheduler] Spawning timer for reminder {}, delay={}s, remind_at={}",
            timer_id,
            delay.as_secs(),
            remind_at
        );

        let note_title_clone = note_title.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;

            tracing::info!("[Scheduler] Reminder {} fired!", timer_id);

            // Mark as triggered on server
            let client = reqwest::Client::new();
            let _ = client
                .post(format!("{}/api/reminders/{}/trigger", server_url, timer_id))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await;

            // Set reminder_pending to start tray flashing
            if let Some(state) = app.try_state::<crate::commands::AppState>() {
                state.reminder_pending.store(true, Ordering::SeqCst);
            }

            // Show desktop notification window
            crate::notification::show_notification(&app, &note_title_clone, &note_body);

            // Also try native OS notification
            use tauri_plugin_notification::NotificationExt;
            let result = app
                .notification()
                .builder()
                .title(&note_title_clone)
                .body(&note_body)
                .show();
            tracing::info!("[Scheduler] Notification result: {:?}", result);
        });

        // Store the handle so we can cancel it if the reminder is deleted
        self.timers
            .lock()
            .unwrap()
            .insert(reminder_id, handle);
    }

    /// Cancel a timer for a deleted reminder.
    pub fn cancel(&self, id: &str) {
        if let Some(handle) = self.timers.lock().unwrap().remove(id) {
            tracing::info!("[Scheduler] Cancelling timer for reminder {}", id);
            handle.abort();
        }
    }
}
