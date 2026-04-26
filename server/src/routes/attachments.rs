use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::{header::AUTHORIZATION, HeaderMap},
};
use minio::s3::segmented_bytes::SegmentedBytes;
use minio::s3::types::S3Api;
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

    // Upload to MinIO using the official minio-rs SDK
    let sb = SegmentedBytes::from(String::from_utf8_lossy(&file_bytes).into_owned());
    state.s3_client
        .put_object(&state.minio_bucket, &s3_key, sb)
        .map_err(|e| AppError::BadRequest(e.to_string()))?
        .build()
        .send()
        .await
        .map_err(|e| AppError::InternalError(format!("上传到MinIO失败: {}", e)))?;

    let url = format!("{}/{}", state.minio_public_url.trim_end_matches('/'), s3_key);
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

    if let Ok(builder) = state.s3_client.delete_object(&state.minio_bucket, attachment.s3_key.as_str()) {
        let _ = builder.build().send().await;
    }

    sqlx::query("DELETE FROM attachments WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(ApiResponse::success(())))
}
