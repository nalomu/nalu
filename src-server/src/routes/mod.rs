pub mod notes;
pub mod schedules;
pub mod sync;
pub mod tasks;

use axum::{Router, routing::get};

use crate::state::SharedState;

/// Public routes — no authentication required
pub fn public_router() -> Router<SharedState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/pair", axum::routing::post(crate::auth::pair_handler))
}

/// Protected routes — require JWT authentication
pub fn protected_router() -> Router<SharedState> {
    Router::new()
        .merge(tasks::router())
        .merge(notes::router())
        .merge(schedules::router())
        .merge(sync::router())
}
