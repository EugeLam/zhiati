use axum::{
    Json, extract::{State, Path},
    http::{header::AUTHORIZATION, HeaderMap},
};
use uuid::Uuid;
use chrono::Utc;

use zhiati_shared::{Note, CreateNoteRequest, UpdateNoteRequest, SyncRequest, SyncResponse, ApiResponse};

use crate::error::AppError;
use crate::AppState;
use crate::routes::auth::verify_token;

fn extract_user_id_from_headers(headers: &HeaderMap, jwt_secret: &str) -> Result<Uuid, AppError> {
    let auth_header = headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match auth_header {
        Some(token) => {
            let user_id = verify_token(token, jwt_secret)
                .map_err(|_| AppError::Unauthorized("Invalid token".to_string()))?;
            user_id.parse()
                .map_err(|_| AppError::BadRequest("Invalid user ID".to_string()))
        }
        None => Err(AppError::Unauthorized("Missing authorization header".to_string())),
    }
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<Note>>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let notes: Vec<Note> = sqlx::query_as(
        r#"
        SELECT id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at
        FROM notes
        WHERE user_id = $1 AND is_archived = false
        ORDER BY is_pinned DESC, updated_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(notes)))
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateNoteRequest>,
) -> Result<Json<ApiResponse<Note>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let note = Note::new(user_id, req.title.clone());
    let color = req.color.unwrap_or_else(|| "#FFFB00".to_string());

    let created: Note = sqlx::query_as(
        r#"
        INSERT INTO notes (user_id, title, content, color)
        VALUES ($1, $2, $3, $4)
        RETURNING id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at
        "#,
    )
    .bind(user_id)
    .bind(&req.title)
    .bind(&note.content)
    .bind(&color)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(created)))
}

pub async fn get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Note>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let note: Option<Note> = sqlx::query_as(
        r#"
        SELECT id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at
        FROM notes
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    match note {
        Some(n) => Ok(Json(ApiResponse::success(n))),
        None => Err(AppError::NotFound("Note not found".to_string())),
    }
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Json<ApiResponse<Note>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let existing: Option<Note> = sqlx::query_as(
        "SELECT id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at FROM notes WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let existing = existing.ok_or_else(|| AppError::NotFound("Note not found".to_string()))?;

    let title = req.title.unwrap_or(existing.title);
    let content = req.content.or(existing.content);
    let is_pinned = req.is_pinned.unwrap_or(existing.is_pinned);
    let is_archived = req.is_archived.unwrap_or(existing.is_archived);
    let color = req.color.unwrap_or(existing.color);

    let updated: Note = sqlx::query_as(
        r#"
        UPDATE notes
        SET title = $1, content = $2, is_pinned = $3, is_archived = $4, color = $5, updated_at = NOW()
        WHERE id = $6 AND user_id = $7
        RETURNING id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at
        "#,
    )
    .bind(&title)
    .bind(&content)
    .bind(is_pinned)
    .bind(is_archived)
    .bind(&color)
    .bind(id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(updated)))
}

pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let keys: Vec<(String,)> = sqlx::query_as(
        "SELECT s3_key FROM attachments WHERE note_id = $1",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    for (s3_key,) in keys {
        let _ = state.s3_client
            .delete_object()
            .bucket(&state.s3_bucket)
            .key(s3_key.as_str())
            .send()
            .await;
    }

    let result = sqlx::query(
        "DELETE FROM notes WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Note not found".to_string()));
    }

    Ok(Json(ApiResponse::success(())))
}

pub async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SyncRequest>,
) -> Result<Json<ApiResponse<SyncResponse>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let synced_at = Utc::now();

    for note in req.notes {
        if note.id == Uuid::nil() {
            let new_note = Note::new(user_id, note.title.clone());
            let _: Note = sqlx::query_as(
                r#"
                INSERT INTO notes (user_id, title, content, is_pinned, color, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at
                "#,
            )
            .bind(user_id)
            .bind(&new_note.title)
            .bind(&note.content)
            .bind(note.is_pinned)
            .bind(&note.color)
            .bind(synced_at)
            .fetch_one(&state.db)
            .await?;
        } else {
            let _: Option<Note> = sqlx::query_as(
                r#"
                UPDATE notes
                SET title = $1, content = $2, is_pinned = $3, color = $4, updated_at = $5, synced_at = $6
                WHERE id = $7 AND user_id = $8
                RETURNING id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at
                "#,
            )
            .bind(&note.title)
            .bind(&note.content)
            .bind(note.is_pinned)
            .bind(&note.color)
            .bind(synced_at)
            .bind(synced_at)
            .bind(note.id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?;
        }
    }

    let all_notes: Vec<Note> = sqlx::query_as(
        r#"
        SELECT id, user_id, title, content, is_pinned, is_archived, color, created_at, updated_at, synced_at
        FROM notes
        WHERE user_id = $1
        ORDER BY updated_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(SyncResponse {
        notes: all_notes,
        synced_at,
    })))
}
