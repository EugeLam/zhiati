use axum::{
    Json, extract::{State, Path, Query},
    http::{header::AUTHORIZATION, HeaderMap},
};
use uuid::Uuid;
use serde::Deserialize;

use zhiati_shared::{Reminder, CreateReminderRequest, UpdateReminderRequest, ApiResponse};

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

#[derive(Debug, Deserialize)]
pub struct ReminderQuery {
    pub note_id: Option<Uuid>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ReminderQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<Reminder>>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let reminders: Vec<Reminder> = if let Some(note_id) = query.note_id {
        sqlx::query_as(
            "SELECT r.id, r.note_id, r.user_id, r.remind_at, r.is_triggered, r.created_at, r.updated_at, \
             n.title as note_title, n.content as note_content \
             FROM reminders r LEFT JOIN notes n ON r.note_id = n.id \
             WHERE r.user_id = $1 AND r.note_id = $2 ORDER BY r.remind_at ASC",
        )
        .bind(user_id)
        .bind(note_id)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT r.id, r.note_id, r.user_id, r.remind_at, r.is_triggered, r.created_at, r.updated_at, \
             n.title as note_title, n.content as note_content \
             FROM reminders r LEFT JOIN notes n ON r.note_id = n.id \
             WHERE r.user_id = $1 AND r.is_triggered = false ORDER BY r.remind_at ASC",
        )
        .bind(user_id)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(ApiResponse::success(reminders)))
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateReminderRequest>,
) -> Result<Json<ApiResponse<Reminder>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let reminder: Reminder = sqlx::query_as(
        r#"
        INSERT INTO reminders (note_id, user_id, remind_at)
        VALUES ($1, $2, $3)
        RETURNING id, note_id, user_id, remind_at, is_triggered, created_at, updated_at,
          (SELECT title FROM notes WHERE id = note_id) as note_title,
          (SELECT content FROM notes WHERE id = note_id) as note_content
        "#,
    )
    .bind(&req.note_id)
    .bind(user_id)
    .bind(&req.remind_at)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(reminder)))
}

pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateReminderRequest>,
) -> Result<Json<ApiResponse<Reminder>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let existing: Option<Reminder> = sqlx::query_as(
        "SELECT id, note_id, user_id, remind_at, is_triggered, created_at, updated_at \
         FROM reminders WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let existing = existing.ok_or_else(|| AppError::NotFound("Reminder not found".to_string()))?;

    let remind_at = req.remind_at.unwrap_or(existing.remind_at);

    let updated: Reminder = sqlx::query_as(
        r#"
        UPDATE reminders SET remind_at = $1, updated_at = NOW()
        WHERE id = $2 AND user_id = $3
        RETURNING id, note_id, user_id, remind_at, is_triggered, created_at, updated_at,
          (SELECT title FROM notes WHERE id = note_id) as note_title,
          (SELECT content FROM notes WHERE id = note_id) as note_content
        "#,
    )
    .bind(&remind_at)
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

    let result = sqlx::query("DELETE FROM reminders WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Reminder not found".to_string()));
    }

    Ok(Json(ApiResponse::success(())))
}

pub async fn mark_triggered(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Reminder>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let reminder: Reminder = sqlx::query_as(
        r#"
        UPDATE reminders SET is_triggered = true, updated_at = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, note_id, user_id, remind_at, is_triggered, created_at, updated_at,
          (SELECT title FROM notes WHERE id = note_id) as note_title,
          (SELECT content FROM notes WHERE id = note_id) as note_content
        "#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => AppError::NotFound("Reminder not found".to_string()),
        other => AppError::DatabaseError(other),
    })?;

    Ok(Json(ApiResponse::success(reminder)))
}
