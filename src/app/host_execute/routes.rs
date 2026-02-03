use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

use super::handlers::*;
use crate::AppState;

// ============================================================================
// Routes
// ============================================================================

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/hosts/{id}/execute", post(execute_command))
        .route("/api/v1/hosts/{id}/info", get(get_host_info))
        .route("/api/v1/hosts/{id}/shell", get(shell_handler))
}
