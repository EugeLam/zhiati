use axum::{
    Json,
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header::AUTHORIZATION, HeaderMap},
    response::Response,
};
use aws_sdk_s3::primitives::ByteStream;
use serde::Deserialize;
use uuid::Uuid;
use zhiati_shared::{ApiResponse, Attachment, AttachmentUploadResponse};

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

fn allowed_image_mime(mime: &str) -> bool {
    mime.starts_with("image/png") || mime.starts_with("image/jpeg") || mime.starts_with("image/gif") || mime.starts_with("image/webp") || mime.starts_with("image/svg")
}

fn extension_from_mime(mime: &str) -> &str {
    if mime.contains("png") { "png" }
    else if mime.contains("gif") { "gif" }
    else if mime.contains("webp") { "webp" }
    else if mime.contains("svg") { "svg" }
    else { "jpg" }
}

pub async fn upload_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<AttachmentUploadResponse>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let mut file_data: Option<(String, String, String, Vec<u8>)> = None;
    let mut note_id_str: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let filename = field.file_name()
                    .map(|s| s.to_string())
                    .ok_or_else(|| AppError::BadRequest("Missing filename".to_string()))?;
                let mime_type = field.content_type()
                    .map(|s| s.to_string())
                    .ok_or_else(|| AppError::BadRequest("Missing content type".to_string()))?;

                if !allowed_image_mime(&mime_type) {
                    return Err(AppError::BadRequest(format!("不支持的文件类型: {}", mime_type)));
                }

                let data = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                if data.len() > state.max_upload_size {
                    return Err(AppError::BadRequest(format!("文件大小超过限制 ({}MB)", state.max_upload_size / 1024 / 1024)));
                }

                let ext = extension_from_mime(&mime_type);
                let s3_key = format!("attachments/{}/{}.{}", user_id, Uuid::new_v4(), ext);

                file_data = Some((s3_key, filename, mime_type, data.to_vec()));
            }
            "note_id" => {
                note_id_str = Some(field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?);
            }
            _ => {}
        }
    }

    let (s3_key, filename, mime_type, file_bytes) = file_data
        .ok_or_else(|| AppError::BadRequest("Missing file".to_string()))?;
    let note_id: Uuid = note_id_str
        .ok_or_else(|| AppError::BadRequest("Missing note_id".to_string()))?
        .parse()
        .map_err(|_| AppError::BadRequest("Invalid note_id".to_string()))?;

    state.s3_client
        .put_object()
        .bucket(&state.s3_bucket)
        .key(&s3_key)
        .body(ByteStream::from(file_bytes.clone()))
        .send()
        .await
        .map_err(|e| AppError::InternalError(format!("上传到RustFS失败: {}", e)))?;

    let url = format!("{}/{}/{}", state.s3_public_url.trim_end_matches('/'), state.s3_bucket, s3_key);
    let file_size = file_bytes.len() as i64;

    let attachment: Attachment = sqlx::query_as(
        r#"
        INSERT INTO attachments (note_id, user_id, filename, mime_type, size, s3_key)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, note_id, user_id, filename, mime_type, size, s3_key, created_at
        "#,
    )
    .bind(note_id)
    .bind(user_id)
    .bind(&filename)
    .bind(&mime_type)
    .bind(file_size)
    .bind(&s3_key)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ApiResponse::success(AttachmentUploadResponse {
        id: attachment.id,
        filename: attachment.filename,
        url,
        size: attachment.size,
    })))
}

pub async fn delete_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    let attachment: Option<Attachment> = sqlx::query_as(
        "SELECT id, note_id, user_id, filename, mime_type, size, s3_key, created_at FROM attachments WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let attachment = attachment
        .ok_or_else(|| AppError::NotFound("Attachment not found".to_string()))?;

    let _ = state.s3_client
        .delete_object()
        .bucket(&state.s3_bucket)
        .key(attachment.s3_key.as_str())
        .send()
        .await;

    sqlx::query("DELETE FROM attachments WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse::success(())))
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    pub s3_key: String,
}

/// Download an attachment by s3_key. Authenticates the user, then streams the file from S3.
pub async fn download_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, AppError> {
    let user_id = extract_user_id_from_headers(&headers, &state.jwt_secret)?;

    // Verify the attachment belongs to this user
    let attachment: Option<Attachment> = sqlx::query_as(
        "SELECT id, note_id, user_id, filename, mime_type, size, s3_key, created_at FROM attachments WHERE s3_key = $1 AND user_id = $2",
    )
    .bind(&query.s3_key)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?;

    let attachment = attachment
        .ok_or_else(|| AppError::NotFound("Attachment not found".to_string()))?;

    // Fetch from S3
    let object = state
        .s3_client
        .get_object()
        .bucket(&state.s3_bucket)
        .key(&attachment.s3_key)
        .send()
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to fetch from S3: {}", e)))?;

    let body_bytes = object
        .body
        .collect()
        .await
        .map_err(|e| AppError::InternalError(format!("Failed to read S3 body: {}", e)))?
        .into_bytes();

    let content_type = attachment.mime_type.unwrap_or_else(|| "application/octet-stream".to_string());

    let mut response = Response::new(Body::from(body_bytes.to_vec()));
    response
        .headers_mut()
        .insert("Content-Type",
            content_type.parse().unwrap_or_else(|_| "application/octet-stream".parse().unwrap()));
    response
        .headers_mut()
        .insert("Content-Disposition",
            format!("inline; filename=\"{}\"", attachment.filename)
                .parse()
                .unwrap_or_else(|_| "inline".parse().unwrap()));
    Ok(response)
}
