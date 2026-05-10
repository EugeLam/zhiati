mod routes;
mod services;
mod error;

use std::env;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{CorsLayer, Any};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use aws_sdk_s3::Client as S3Client;
use aws_credential_types::Credentials;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_secret: String,
    pub s3_client: S3Client,
    pub s3_bucket: String,
    pub s3_public_url: String,
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

    let s3_endpoint = env::var("RUSTFS_ENDPOINT_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9000".into());
    let s3_access_key = env::var("RUSTFS_ACCESS_KEY_ID")
        .unwrap_or_else(|_| "minioadmin".into());
    let s3_secret_key = env::var("RUSTFS_SECRET_ACCESS_KEY")
        .unwrap_or_else(|_| "minioadmin".into());
    let s3_region = env::var("RUSTFS_REGION")
        .unwrap_or_else(|_| "us-east-1".into());
    let s3_bucket_name = env::var("RUSTFS_BUCKET")
        .unwrap_or_else(|_| "zhiati-attachments".into());
    let s3_public_url = env::var("RUSTFS_PUBLIC_URL")
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

    // Setup S3 client for RustFS
    let credentials = Credentials::new(&s3_access_key, &s3_secret_key, None, None, "rustfs");
    let region = aws_config::Region::new(s3_region);

    let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region)
        .credentials_provider(credentials)
        .endpoint_url(&s3_endpoint)
        .load()
        .await;

    let s3_client = S3Client::new(&shared_config);

    // Ensure bucket exists
    tracing::info!("Checking S3 bucket '{}'...", s3_bucket_name);
    let resp = s3_client.list_buckets().send().await;

    match resp {
        Ok(res) => {
            let bucket_exists = res.buckets().iter().any(|b| b.name().as_deref() == Some(&s3_bucket_name));
            if !bucket_exists {
                tracing::info!("Creating S3 bucket '{}'...", s3_bucket_name);
                s3_client.create_bucket()
                    .bucket(&s3_bucket_name)
                    .send()
                    .await
                    .map_err(|e| format!("Failed to create bucket: {}", e))?;
                tracing::info!("S3 bucket '{}' created", s3_bucket_name);
            } else {
                tracing::info!("S3 bucket '{}' ready", s3_bucket_name);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to check S3 bucket: {}. Server will continue — upload will fail until RustFS is configured.", e);
        }
    }

    let state = AppState {
        db: pool,
        jwt_secret,
        s3_client,
        s3_bucket: s3_bucket_name,
        s3_public_url,
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
