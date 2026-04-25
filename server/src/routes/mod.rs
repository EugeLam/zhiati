pub mod auth;
pub mod notes;

use axum::Router;
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
