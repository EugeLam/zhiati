use chrono::{DateTime, Utc};
use sqlx::{SqlitePool, sqlite::{SqliteRow, SqliteConnectOptions}, Row};
use shared::{Note, Reminder};
use uuid::Uuid;
use std::path::Path;
use std::str::FromStr;

pub async fn init_db(db_path: &Path) -> Result<SqlitePool, String> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create DB directory: {}", e))?;
    }
    // Use SqliteConnectOptions for reliable path handling on Windows
    let options = SqliteConnectOptions::from_str(&db_path.to_str().ok_or("Invalid DB path")?)
        .map_err(|e| format!("Invalid DB path: {}", e))?
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(options)
        .await
        .map_err(|e| format!("Failed to connect to SQLite: {}", e))?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS notes (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL,
            content TEXT,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            is_archived INTEGER NOT NULL DEFAULT 0,
            color TEXT NOT NULL DEFAULT '#FFFB00',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            synced_at TEXT
        )"#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to create notes table: {}", e))?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS reminders (
            id TEXT PRIMARY KEY,
            note_id TEXT NOT NULL,
            user_id TEXT NOT NULL DEFAULT '',
            remind_at TEXT NOT NULL,
            is_triggered INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            note_title TEXT,
            note_content TEXT,
            FOREIGN KEY (note_id) REFERENCES notes(id)
        )"#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to create reminders table: {}", e))?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS tags (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL DEFAULT '',
            name TEXT NOT NULL,
            color TEXT NOT NULL,
            created_at TEXT NOT NULL
        )"#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to create tags table: {}", e))?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS note_tags (
            note_id TEXT NOT NULL,
            tag_id TEXT NOT NULL,
            PRIMARY KEY (note_id, tag_id),
            FOREIGN KEY (note_id) REFERENCES notes(id),
            FOREIGN KEY (tag_id) REFERENCES tags(id)
        )"#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to create note_tags table: {}", e))?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS sync_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )"#,
    )
    .execute(&pool)
    .await
    .map_err(|e| format!("Failed to create sync_metadata table: {}", e))?;

    // Enable WAL mode for better concurrent read performance
    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

    // NORMAL synchronous mode: fsync only at critical points, much faster writes
    // with WAL this is safe against corruption
    sqlx::query("PRAGMA synchronous=NORMAL")
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set synchronous mode: {}", e))?;

    // Larger cache: 16MB (default is ~2MB)
    sqlx::query("PRAGMA cache_size=-16384")
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set cache size: {}", e))?;

    // Wait up to 5s for locked database instead of failing immediately
    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&pool)
        .await
        .map_err(|e| format!("Failed to set busy timeout: {}", e))?;

    Ok(pool)
}

fn row_to_note(row: &SqliteRow) -> Note {
    let created_at: DateTime<Utc> = row.get::<String, _>("created_at").parse().unwrap_or(Utc::now());
    let updated_at: DateTime<Utc> = row.get::<String, _>("updated_at").parse().unwrap_or(Utc::now());
    let synced_at: Option<DateTime<Utc>> = row
        .get::<Option<String>, _>("synced_at")
        .and_then(|s| s.parse().ok());

    Note {
        id: Uuid::parse_str(&row.get::<String, _>("id")).unwrap_or(Uuid::nil()),
        user_id: Uuid::parse_str(&row.get::<String, _>("user_id")).unwrap_or(Uuid::nil()),
        title: row.get::<String, _>("title"),
        content: row.get::<Option<String>, _>("content"),
        is_pinned: row.get::<i64, _>("is_pinned") != 0,
        is_archived: row.get::<i64, _>("is_archived") != 0,
        color: row.get::<String, _>("color"),
        created_at,
        updated_at,
        synced_at,
    }
}

fn row_to_reminder(row: &SqliteRow) -> Reminder {
    let created_at: DateTime<Utc> = row.get::<String, _>("created_at").parse().unwrap_or(Utc::now());
    let updated_at: DateTime<Utc> = row.get::<String, _>("updated_at").parse().unwrap_or(Utc::now());
    let remind_at: DateTime<Utc> = row.get::<String, _>("remind_at").parse().unwrap_or(Utc::now());

    Reminder {
        id: Uuid::parse_str(&row.get::<String, _>("id")).unwrap_or(Uuid::nil()),
        note_id: Uuid::parse_str(&row.get::<String, _>("note_id")).unwrap_or(Uuid::nil()),
        user_id: Uuid::parse_str(&row.get::<String, _>("user_id")).unwrap_or(Uuid::nil()),
        remind_at,
        is_triggered: row.get::<i64, _>("is_triggered") != 0,
        created_at,
        updated_at,
        note_title: row.get::<Option<String>, _>("note_title"),
        note_content: row.get::<Option<String>, _>("note_content"),
    }
}

pub async fn get_notes(pool: &SqlitePool) -> Result<Vec<Note>, String> {
    let rows = sqlx::query("SELECT * FROM notes ORDER BY updated_at DESC")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to fetch notes: {}", e))?;
    Ok(rows.iter().map(row_to_note).collect())
}

pub async fn get_note_by_id(pool: &SqlitePool, id: &str) -> Result<Option<Note>, String> {
    let row = sqlx::query("SELECT * FROM notes WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to fetch note: {}", e))?;
    Ok(row.map(|r| row_to_note(&r)))
}

pub async fn create_note(pool: &SqlitePool, note: &Note) -> Result<(), String> {
    sqlx::query(
        r#"INSERT INTO notes (id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(note.id.to_string())
    .bind(note.user_id.to_string())
    .bind(&note.title)
    .bind(&note.content)
    .bind(if note.is_pinned { 1 } else { 0 })
    .bind(if note.is_archived { 1 } else { 0 })
    .bind(&note.color)
    .bind(note.created_at.to_rfc3339())
    .bind(note.updated_at.to_rfc3339())
    .bind(note.synced_at.map(|dt| dt.to_rfc3339()))
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create note: {}", e))?;
    Ok(())
}

pub async fn update_note(pool: &SqlitePool, note: &Note) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE notes SET title = ?, content = ?, is_pinned = ?, is_archived = ?, color = ?, updated_at = ?, synced_at = ? WHERE id = ?"#,
    )
    .bind(&note.title)
    .bind(&note.content)
    .bind(if note.is_pinned { 1 } else { 0 })
    .bind(if note.is_archived { 1 } else { 0 })
    .bind(&note.color)
    .bind(note.updated_at.to_rfc3339())
    .bind(note.synced_at.map(|dt| dt.to_rfc3339()))
    .bind(note.id.to_string())
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update note: {}", e))?;
    Ok(())
}

pub async fn upsert_note(pool: &SqlitePool, note: &Note) -> Result<(), String> {
    sqlx::query(
        r#"INSERT INTO notes (id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(id) DO UPDATE SET title=excluded.title, content=excluded.content,
               is_pinned=excluded.is_pinned, is_archived=excluded.is_archived,
               color=excluded.color, updated_at=excluded.updated_at, synced_at=excluded.synced_at"#,
    )
    .bind(note.id.to_string())
    .bind(note.user_id.to_string())
    .bind(&note.title)
    .bind(&note.content)
    .bind(if note.is_pinned { 1 } else { 0 })
    .bind(if note.is_archived { 1 } else { 0 })
    .bind(&note.color)
    .bind(note.created_at.to_rfc3339())
    .bind(note.updated_at.to_rfc3339())
    .bind(note.synced_at.map(|dt| dt.to_rfc3339()))
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to upsert note: {}", e))?;
    Ok(())
}

pub async fn delete_note(pool: &SqlitePool, id: &str) -> Result<(), String> {
    // Also delete related reminders and note_tags
    sqlx::query("DELETE FROM note_tags WHERE note_id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete note tags: {}", e))?;
    sqlx::query("DELETE FROM reminders WHERE note_id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete related reminders: {}", e))?;
    sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete note: {}", e))?;
    Ok(())
}

pub async fn get_reminders(pool: &SqlitePool, note_id: Option<&str>) -> Result<Vec<Reminder>, String> {
    let rows = match note_id {
        Some(nid) => sqlx::query("SELECT * FROM reminders WHERE note_id = ? ORDER BY remind_at ASC")
            .bind(nid)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("Failed to fetch reminders: {}", e))?,
        None => sqlx::query("SELECT * FROM reminders ORDER BY remind_at ASC")
            .fetch_all(pool)
            .await
            .map_err(|e| format!("Failed to fetch all reminders: {}", e))?,
    };
    Ok(rows.iter().map(row_to_reminder).collect())
}

pub async fn create_reminder(pool: &SqlitePool, reminder: &Reminder) -> Result<(), String> {
    sqlx::query(
        r#"INSERT INTO reminders (id, note_id, user_id, remind_at, is_triggered, created_at, updated_at, note_title, note_content)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(reminder.id.to_string())
    .bind(reminder.note_id.to_string())
    .bind(reminder.user_id.to_string())
    .bind(reminder.remind_at.to_rfc3339())
    .bind(if reminder.is_triggered { 1 } else { 0 })
    .bind(reminder.created_at.to_rfc3339())
    .bind(reminder.updated_at.to_rfc3339())
    .bind(&reminder.note_title)
    .bind(&reminder.note_content)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create reminder: {}", e))?;
    Ok(())
}

pub async fn delete_reminder(pool: &SqlitePool, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM reminders WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete reminder: {}", e))?;
    Ok(())
}

pub async fn get_last_synced_at(pool: &SqlitePool) -> Result<Option<DateTime<Utc>>, String> {
    let row = sqlx::query("SELECT value FROM sync_metadata WHERE key = 'last_synced_at'")
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to fetch sync metadata: {}", e))?;
    Ok(row.and_then(|r| r.get::<String, _>("value").parse().ok()))
}

pub async fn set_last_synced_at(pool: &SqlitePool, dt: DateTime<Utc>) -> Result<(), String> {
    sqlx::query(
        r#"INSERT INTO sync_metadata (key, value) VALUES ('last_synced_at', ?)
           ON CONFLICT(key) DO UPDATE SET value = excluded.value"#,
    )
    .bind(dt.to_rfc3339())
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to set sync metadata: {}", e))?;
    Ok(())
}

pub async fn upsert_all_notes(pool: &SqlitePool, notes: &[Note]) -> Result<(), String> {
    for note in notes {
        upsert_note(pool, note).await?;
    }
    Ok(())
}
