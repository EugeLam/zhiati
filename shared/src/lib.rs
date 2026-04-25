use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Note {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub content: Option<String>,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub color: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub synced_at: Option<DateTime<Utc>>,
}

impl Note {
    pub fn new(user_id: Uuid, title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            title,
            content: None,
            is_pinned: false,
            is_archived: false,
            color: "#FFFB00".to_string(),
            created_at: now,
            updated_at: now,
            synced_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tag {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub color: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteTag {
    pub note_id: Uuid,
    pub tag_id: Uuid,
}

// API request/response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub title: String,
    pub content: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub is_pinned: Option<bool>,
    pub is_archived: Option<bool>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub notes: Vec<Note>,
    pub last_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub notes: Vec<Note>,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(msg: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg),
        }
    }
}

// CLI output types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliNoteOutput {
    pub id: String,
    pub title: String,
    pub content: Option<String>,
    pub is_pinned: bool,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Note> for CliNoteOutput {
    fn from(note: Note) -> Self {
        Self {
            id: note.id.to_string(),
            title: note.title,
            content: note.content,
            is_pinned: note.is_pinned,
            color: note.color,
            created_at: note.created_at.to_rfc3339(),
            updated_at: note.updated_at.to_rfc3339(),
        }
    }
}
