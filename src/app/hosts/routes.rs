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
        // 主机 CRUD
        .route("/api/v1/hosts", get(get_hosts))
        .route("/api/v1/hosts", post(create_host))
        .route("/api/v1/hosts/{id}", get(get_host))
        .route("/api/v1/hosts/{id}", put(update_host))
        .route("/api/v1/hosts/{id}", delete(delete_host))
        // 主机操作
        .route("/api/v1/hosts/{id}/test", post(test_host))
        .route("/api/v1/hosts/batch/test", post(batch_test_hosts))
        .route("/api/v1/hosts/batch/delete", post(batch_delete_hosts))
        // 导入导出
        .route("/api/v1/hosts/import", post(import_hosts))
        .route("/api/v1/hosts/export", get(export_hosts))
        // 工具
        .route("/api/v1/hosts/default-key-path", get(get_default_key_path))
}
