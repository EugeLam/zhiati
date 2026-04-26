mod routes;
mod services;
mod error;

use std::env;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{CorsLayer, Any};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use minio::s3::MinioClient;
use minio::s3::creds::StaticProvider;
use minio::s3::http::BaseUrl;
use minio::s3::types::S3Api;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_secret: String,
    pub s3_client: MinioClient,
    pub minio_bucket: String,
    pub minio_public_url: String,
    pub max_upload_size: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let jwt_secret = env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set");

    let minio_endpoint = env::var("MINIO_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:9000".into());
    // Ensure trailing slash for proper V4 signing
    let minio_endpoint = if minio_endpoint.ends_with('/') {
        minio_endpoint
    } else {
        format!("{}/", minio_endpoint)
    };
    let minio_access_key = env::var("MINIO_ACCESS_KEY")
        .unwrap_or_else(|_| "minioadmin".into());
    let minio_secret_key = env::var("MINIO_SECRET_KEY")
        .unwrap_or_else(|_| "minioadmin".into());
    let minio_bucket_name = env::var("MINIO_BUCKET")
        .unwrap_or_else(|_| "zhiati-attachments".into());
    let minio_public_url = env::var("MINIO_PUBLIC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9000".into());
    let max_upload_size: usize = env::var("MAX_UPLOAD_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10 * 1024 * 1024);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            email VARCHAR(255) UNIQUE NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS notes (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            user_id UUID REFERENCES users(id) ON DELETE SET NULL,
            title VARCHAR(255) NOT NULL,
            content TEXT,
            is_pinned BOOLEAN DEFAULT FALSE,
            is_archived BOOLEAN DEFAULT FALSE,
            color VARCHAR(20) DEFAULT '#FFFB00',
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            synced_at TIMESTAMP WITH TIME ZONE
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tags (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            user_id UUID REFERENCES users(id) ON DELETE CASCADE,
            name VARCHAR(50) NOT NULL,
            color VARCHAR(20) DEFAULT '#808080',
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS note_tags (
            note_id UUID REFERENCES notes(id) ON DELETE CASCADE,
            tag_id UUID REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (note_id, tag_id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS reminders (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            note_id UUID NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            remind_at TIMESTAMPTZ NOT NULL,
            is_triggered BOOLEAN DEFAULT FALSE,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            updated_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS attachments (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            note_id UUID NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            filename VARCHAR(255) NOT NULL,
            mime_type VARCHAR(100),
            size BIGINT NOT NULL,
            s3_key VARCHAR(500) NOT NULL,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Setup MinIO client
    let base_url = minio_endpoint.parse::<BaseUrl>()
        .map_err(|e| format!("Invalid MinIO endpoint: {}", e))?;

    let provider = StaticProvider::new(&minio_access_key, &minio_secret_key, None);

    let s3_client = MinioClient::new(base_url, Some(provider), None, Some(true))
        .map_err(|e| format!("Failed to create MinIO client: {}", e))?;

    // Ensure bucket exists
    tracing::info!("Checking MinIO bucket '{}'...", minio_bucket_name);
    let resp = s3_client.bucket_exists(&minio_bucket_name)?
        .build()
        .send()
        .await
        .map_err(|e| format!("Failed to check bucket: {}", e))?;

    if !resp.exists() {
        tracing::info!("Creating MinIO bucket '{}'...", minio_bucket_name);
        s3_client.create_bucket(&minio_bucket_name)?
            .build()
            .send()
            .await
            .map_err(|e| format!("Failed to create bucket: {}", e))?;
        tracing::info!("MinIO bucket '{}' created", minio_bucket_name);
    } else {
        tracing::info!("MinIO bucket '{}' ready", minio_bucket_name);
    }

    let state = AppState {
        db: pool,
        jwt_secret,
        s3_client,
        minio_bucket: minio_bucket_name,
        minio_public_url,
        max_upload_size,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .nest("/api/auth", routes::auth_routes())
        .nest("/api/notes", routes::notes_routes())
        .nest("/api/reminders", routes::reminders_routes())
        .nest("/api/attachments", routes::attachments_routes())
        .route("/health", axum::routing::get(health_check))
        .layer(cors)
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Server running on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
