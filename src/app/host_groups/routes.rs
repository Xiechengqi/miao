use axum::routing::{get, post, put, delete};
use axum::Router;
use std::sync::Arc;

use super::handlers::*;
use crate::AppState;

// ============================================================================
// Routes
// ============================================================================

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/host-groups", get(list_groups))
        .route("/api/v1/host-groups", post(create_group))
        .route("/api/v1/host-groups/{id}", get(get_group))
        .route("/api/v1/host-groups/{id}", put(update_group))
        .route("/api/v1/host-groups/{id}", delete(delete_group))
        .route("/api/v1/host-groups/{id}/hosts", put(update_group_hosts))
}
