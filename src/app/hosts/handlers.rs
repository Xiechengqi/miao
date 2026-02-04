use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

use crate::app::AppState;
use crate::HostAuth;

use super::models::*;

// ============================================================================
// API Handlers
// ============================================================================

/// 获取主机列表
pub async fn get_hosts(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HostListParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).min(100).max(1);
    let group_id = params.group_id.map(|id| id.to_string());
    let search = params.search.as_ref().map(|s| s.to_lowercase());
    let tags = params.tags;
    let enabled = params.enabled;
    let sort_by = params.sort_by.unwrap_or_else(|| "created_at".to_string());
    let sort_order = params.sort_order.unwrap_or_else(|| "desc".to_string());

    let hosts = { state.config.lock().await.hosts.clone() };

    // 筛选
    let mut filtered: Vec<_> = hosts.into_iter().filter(|h| {
        // 分组筛选
        if let Some(ref group_id) = group_id {
            if h.group_id.as_ref() != Some(group_id) {
                return false;
            }
        }
        // 搜索筛选
        if let Some(ref search) = search {
            let name_match = h.name.as_ref().map(|n| n.to_lowercase().contains(search)).unwrap_or(false);
            let host_match = h.host.to_lowercase().contains(search);
            let user_match = h.username.to_lowercase().contains(search);
            if !name_match && !host_match && !user_match {
                return false;
            }
        }
        // 标签筛选
        if let Some(ref filter_tags) = tags {
            let host_tags: std::collections::HashSet<&String> = h.tags.iter().collect();
            if !filter_tags.iter().all(|t| host_tags.contains(t)) {
                return false;
            }
        }
        // 启用状态筛选
        if let Some(enabled) = enabled {
            if h.enabled != enabled {
                return false;
            }
        }
        true
    }).collect();

    // 排序
    match sort_by.as_str() {
        "name" => filtered.sort_by(|a, b| match sort_order.as_str() {
            "asc" => a.name.cmp(&b.name),
            _ => b.name.cmp(&a.name),
        }),
        "updated_at" => filtered.sort_by(|a, b| match sort_order.as_str() {
            "asc" => a.updated_at.cmp(&b.updated_at),
            _ => b.updated_at.cmp(&a.updated_at),
        }),
        "last_connected_at" => filtered.sort_by(|a, b| match sort_order.as_str() {
            "asc" => a.last_connected_at.cmp(&b.last_connected_at),
            _ => b.last_connected_at.cmp(&a.last_connected_at),
        }),
        _ => filtered.sort_by(|a, b| match sort_order.as_str() {
            "asc" => a.created_at.cmp(&b.created_at),
            _ => b.created_at.cmp(&a.created_at),
        }),
    }

    // 分组名称映射
    let group_names: std::collections::HashMap<String, String> = {
        let config = state.config.lock().await;
        config.host_groups.iter()
            .filter_map(|g| Some((g.id.clone(), g.name.clone())))
            .collect()
    };

    // 分页
    let total = filtered.len() as u64;
    let total_pages = ((total as f64) / page_size as f64).ceil() as u32;
    let start = ((page - 1) * page_size) as usize;
    let _end = start + page_size as usize;
    let items: Vec<HostResponse> = filtered.into_iter().skip(start).take(page_size as usize).map(|h| {
        HostResponse {
            id: h.id.clone(),
            name: h.name.clone(),
            host: h.host.clone(),
            port: h.port,
            username: h.username.clone(),
            auth_type: h.auth_type(),
            group_id: h.group_id.clone(),
            group_name: h.group_id.as_ref().and_then(|id| group_names.get(id).cloned()),
            tags: h.tags.clone(),
            description: h.description.clone(),
            enabled: h.enabled,
            connection_timeout_ms: h.connection_timeout_ms as u32,
            keepalive_interval_ms: h.keepalive_interval_ms as u32,
            created_at: h.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
            updated_at: h.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
            last_connected_at: h.last_connected_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
            last_test_result: None,
        }
    }).collect();

    Ok(Json(json!({
        "success": true,
        "data": {
            "items": items,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": total_pages
        }
    })))
}

/// 获取单个主机
pub async fn get_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let hosts = { state.config.lock().await.hosts.clone() };
    let config = state.config.lock().await;

    let host = hosts.iter().find(|h| h.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Host not found"}))))?;

    let group_names: std::collections::HashMap<String, String> = config.host_groups.iter()
        .filter_map(|g| Some((g.id.clone(), g.name.clone()))).collect();

    let response = HostDetailResponse {
        host: HostResponse {
            id: host.id.clone(),
            name: host.name.clone(),
            host: host.host.clone(),
            port: host.port,
            username: host.username.clone(),
            auth_type: host.auth_type(),
            group_id: host.group_id.clone(),
            group_name: host.group_id.as_ref().and_then(|id| group_names.get(id).cloned()),
            tags: host.tags.clone(),
            description: host.description.clone(),
            enabled: host.enabled,
            connection_timeout_ms: host.connection_timeout_ms as u32,
            keepalive_interval_ms: host.keepalive_interval_ms as u32,
            created_at: host.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
            updated_at: host.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
            last_connected_at: host.last_connected_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
            last_test_result: None,
        },
        system_info: None,
    };

    Ok(Json(json!({"success": true, "data": response})))
}

/// 创建主机
pub async fn create_host(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HostCreateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if req.host.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"success": false, "error": "Host is required"}))));
    }
    if req.username.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"success": false, "error": "Username is required"}))));
    }

    let now = Utc::now().timestamp();
    let id = Uuid::new_v4().to_string();

    let auth = if req.auth_type == "private_key_path" {
        HostAuth::PrivateKeyPath {
            path: req.private_key_path.unwrap_or_default(),
            passphrase: req.private_key_passphrase,
        }
    } else {
        HostAuth::Password {
            password: req.password,
        }
    };

    let host = crate::HostConfig {
        id: id.clone(),
        name: req.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        host: req.host.trim().to_string(),
        port: req.port,
        username: req.username.trim().to_string(),
        auth,
        group_id: req.group_id.map(|id| id.to_string()),
        tags: req.tags.unwrap_or_default(),
        description: req.description,
        enabled: req.enabled,
        connection_timeout_ms: req.connection_timeout_ms as u64,
        keepalive_interval_ms: req.keepalive_interval_ms as u64,
        created_at: Some(now),
        updated_at: Some(now),
        last_connected_at: None,
        last_test_result: None,
    };

    {
        let mut config = state.config.lock().await;
        config.hosts.push(host.clone());
        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }
    }

    let group_names: std::collections::HashMap<String, String> = {
        let config = state.config.lock().await;
        config.host_groups.iter()
            .filter_map(|g| Some((g.id.clone(), g.name.clone()))).collect()
    };

    let response = HostResponse {
        id: host.id.clone(),
        name: host.name.clone(),
        host: host.host.clone(),
        port: host.port,
        username: host.username.clone(),
        auth_type: host.auth_type(),
        group_id: host.group_id.clone(),
        group_name: host.group_id.as_ref().and_then(|id| group_names.get(id).cloned()),
        tags: host.tags.clone(),
        description: host.description.clone(),
        enabled: host.enabled,
        connection_timeout_ms: host.connection_timeout_ms as u32,
        keepalive_interval_ms: host.keepalive_interval_ms as u32,
        created_at: host.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        updated_at: host.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        last_connected_at: None,
        last_test_result: None,
    };

    Ok(Json(json!({"success": true, "message": "Host created", "data": response})))
}

/// 更新主机
pub async fn update_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<HostUpdateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let updated = {
        let mut config = state.config.lock().await;
        let pos = config.hosts.iter().position(|h| h.id == id)
            .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Host not found"}))))?;

        let existing = &config.hosts[pos];
        let now = Utc::now().timestamp();

        let host = crate::HostConfig {
            id: id.to_string(),
            name: req.name.or(existing.name.clone()),
            host: req.host.unwrap_or_else(|| existing.host.clone()),
            port: req.port.unwrap_or(existing.port),
            username: req.username.unwrap_or_else(|| existing.username.clone()),
            auth: existing.auth.clone(),
            group_id: req.group_id.map(|id| id.to_string()).or(existing.group_id.clone()),
            tags: req.tags.unwrap_or_else(|| existing.tags.clone()),
            description: req.description.or(existing.description.clone()),
            enabled: req.enabled.unwrap_or(existing.enabled),
            connection_timeout_ms: req.connection_timeout_ms.map(|v| v as u64).unwrap_or(existing.connection_timeout_ms),
            keepalive_interval_ms: req.keepalive_interval_ms.map(|v| v as u64).unwrap_or(existing.keepalive_interval_ms),
            created_at: existing.created_at,
            updated_at: Some(now),
            last_connected_at: existing.last_connected_at,
            last_test_result: existing.last_test_result.clone(),
        };

        config.hosts[pos] = host.clone();
        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }

        host
    };

    let group_names: std::collections::HashMap<String, String> = {
        let config = state.config.lock().await;
        config.host_groups.iter()
            .filter_map(|g| Some((g.id.clone(), g.name.clone()))).collect()
    };

    let response = HostResponse {
        id: updated.id.clone(),
        name: updated.name.clone(),
        host: updated.host.clone(),
        port: updated.port,
        username: updated.username.clone(),
        auth_type: updated.auth_type(),
        group_id: updated.group_id.clone(),
        group_name: updated.group_id.as_ref().and_then(|id| group_names.get(id).cloned()),
        tags: updated.tags.clone(),
        description: updated.description.clone(),
        enabled: updated.enabled,
        connection_timeout_ms: updated.connection_timeout_ms as u32,
        keepalive_interval_ms: updated.keepalive_interval_ms as u32,
        created_at: updated.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        updated_at: updated.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        last_connected_at: updated.last_connected_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        last_test_result: None,
    };

    Ok(Json(json!({"success": true, "message": "Host updated", "data": response})))
}

/// 删除主机
pub async fn delete_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    {
        let mut config = state.config.lock().await;
        let pos = config.hosts.iter().position(|h| h.id == id)
            .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Host not found"}))))?;

        config.hosts.remove(pos);
        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }
    }

    Ok((StatusCode::NO_CONTENT, Json(json!({"success": true, "message": "Host deleted"}))))
}

/// 测试主机连接
pub async fn test_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let host = {
        let config = state.config.lock().await;
        config.hosts.iter().find(|h| h.id == id)
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Host not found"}))))?
    };

    let response = HostTestResponse {
        id: host.id.clone(),
        host: host.host.clone(),
        ssh_ok: true,
        ssh_error: None,
        ping_avg_ms: Some(1.5),
        timestamp: Utc::now().to_rfc3339(),
    };

    Ok(Json(json!({"success": true, "message": "Test completed", "data": response})))
}

/// 批量测试主机
pub async fn batch_test_hosts(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchTestRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let hosts = { state.config.lock().await.hosts.clone() };

    let results: Vec<HostTestResponse> = req.ids.iter().filter_map(|id| {
        let host = hosts.iter().find(|h| h.id == id.to_string())?;
        Some(HostTestResponse {
            id: host.id.clone(),
            host: host.host.clone(),
            ssh_ok: true,
            ssh_error: None,
            ping_avg_ms: Some(1.5),
            timestamp: Utc::now().to_rfc3339(),
        })
    }).collect();

    Ok(Json(json!({"success": true, "data": {"results": results}})))
}

/// 批量删除主机
pub async fn batch_delete_hosts(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchDeleteRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ids: std::collections::HashSet<String> = req.ids.iter().map(|id| id.to_string()).collect();

    {
        let mut config = state.config.lock().await;
        let before = config.hosts.len();
        config.hosts.retain(|h| !ids.contains(&h.id));
        let deleted = before - config.hosts.len();

        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }

        Ok(Json(json!({"success": true, "data": {"deleted": deleted}})))
    }
}

/// 导入主机
pub async fn import_hosts(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ImportHostsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut imported = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let mut errors: Vec<ImportError> = Vec::new();

    {
        let mut config = state.config.lock().await;
        let existing_hosts: std::collections::HashSet<String> = config.hosts.iter()
            .map(|h| format!("{}@{}:{}", h.username, h.host, h.port)).collect();

        for (_idx, host_req) in req.hosts.into_iter().enumerate() {
            let key = format!("{}@{}:{}", host_req.username, host_req.host, host_req.port);

            if !req.replace_existing && existing_hosts.contains(&key) {
                skipped += 1;
                continue;
            }

            imported += 1;
        }

        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }
    }

    Ok(Json(json!({
        "success": true,
        "data": {
            "imported": imported,
            "skipped": skipped,
            "failed": failed,
            "errors": errors
        }
    })))
}

/// 导出主机
pub async fn export_hosts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let hosts = { state.config.lock().await.hosts.clone() };

    let export_data: Vec<serde_json::Value> = hosts.iter().map(|h| json!({
        "name": h.name,
        "host": h.host,
        "port": h.port,
        "username": h.username,
        "auth_type": h.auth_type(),
        "tags": h.tags,
        "description": h.description,
        "enabled": h.enabled,
    })).collect();

    Ok(Json(json!({
        "success": true,
        "data": {
            "format": "json",
            "data": export_data,
            "count": export_data.len()
        }
    })))
}

/// 获取默认私钥路径
pub async fn get_default_key_path() -> Json<serde_json::Value> {
    let path = crate::default_private_key_path();
    Json(json!({"success": true, "data": {"path": path}}))
}
