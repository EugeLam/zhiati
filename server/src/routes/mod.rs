pub mod auth;
pub mod notes;
pub mod reminders;
pub mod attachments;

use axum::{Router, extract::DefaultBodyLimit};
use crate::AppState;

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/register", axum::routing::post(auth::register))
        .route("/login", axum::routing::post(auth::login))
}

pub fn notes_routes() -> Router<AppState> {
    Router::new()
        .route("/", axum::routing::get(notes::list))
        .route("/", axum::routing::post(notes::create))
        .route("/:id", axum::routing::get(notes::get))
        .route("/:id", axum::routing::put(notes::update))
        .route("/:id", axum::routing::delete(notes::delete))
        .route("/sync", axum::routing::post(notes::sync))
}

pub fn reminders_routes() -> Router<AppState> {
    Router::new()
        .route("/", axum::routing::get(reminders::list))
        .route("/", axum::routing::post(reminders::create))
        .route("/:id", axum::routing::put(reminders::update))
        .route("/:id", axum::routing::delete(reminders::delete))
        .route("/:id/trigger", axum::routing::post(reminders::mark_triggered))
}

pub fn attachments_routes() -> Router<AppState> {
    Router::new()
        .route("/upload", axum::routing::post(attachments::upload_attachment)
            .layer(DefaultBodyLimit::max(20 * 1024 * 1024)))
        .route("/download", axum::routing::get(attachments::download_attachment))
        .route("/:id", axum::routing::delete(attachments::delete_attachment))
}