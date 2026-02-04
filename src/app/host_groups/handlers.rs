use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

use crate::app::AppState;
use crate::HostGroupConfig;

use super::models::*;

// ============================================================================
// API Handlers
// ============================================================================

/// 获取分组列表
pub async fn list_groups(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let groups = { state.config.lock().await.host_groups.clone() };
    let hosts = { state.config.lock().await.hosts.clone() };

    let parent_names: std::collections::HashMap<String, String> = {
        let config = state.config.lock().await;
        config.host_groups.iter()
            .filter_map(|g| Some((g.id.clone(), g.name.clone())))
            .collect()
    };

    let host_counts: std::collections::HashMap<String, usize> = {
        let mut counts = std::collections::HashMap::new();
        for h in &hosts {
            if let Some(ref gid) = h.group_id {
                *counts.entry(gid.clone()).or_insert(0) += 1;
            }
        }
        counts
    };

    let group_responses: Vec<HostGroupResponse> = groups.into_iter().map(|g| {
        HostGroupResponse {
            id: g.id.clone(),
            name: g.name.clone(),
            parent_id: g.parent_id.clone(),
            parent_name: g.parent_id.as_ref().and_then(|id| parent_names.get(id).cloned()),
            description: g.description.clone(),
            host_count: host_counts.get(&g.id).cloned().unwrap_or(0),
            created_at: g.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
            updated_at: g.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        }
    }).collect();

    Ok(Json(json!({"success": true, "data": group_responses})))
}

/// 获取分组详情
pub async fn get_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let group = {
        let config = state.config.lock().await;
        config.host_groups.iter().find(|g| g.id == id.to_string())
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Group not found"}))))?
    };

    let parent_names: std::collections::HashMap<String, String> = {
        let config = state.config.lock().await;
        config.host_groups.iter()
            .filter_map(|g| Some((g.id.clone(), g.name.clone())))
            .collect()
    };

    let hosts: Vec<crate::app::hosts::models::HostResponse> = {
        let config = state.config.lock().await;
        config.hosts.iter().filter(|h| h.group_id.as_ref() == Some(&group.id))
            .map(|h| crate::app::hosts::models::HostResponse {
                id: h.id.clone(),
                name: h.name.clone(),
                host: h.host.clone(),
                port: h.port,
                username: h.username.clone(),
                auth_type: h.auth_type(),
                private_key_path: h.private_key_path(),
                group_id: h.group_id.clone(),
                group_name: Some(group.name.clone()),
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
                last_test_result: h.last_test_result.clone(),
            })
            .collect()
    };

    let group_response = HostGroupResponse {
        id: group.id.clone(),
        name: group.name.clone(),
        parent_id: group.parent_id.clone(),
        parent_name: group.parent_id.as_ref().and_then(|id| parent_names.get(id).cloned()),
        description: group.description.clone(),
        host_count: hosts.len(),
        created_at: group.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        updated_at: group.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
    };

    let response = HostGroupDetailResponse {
        group: group_response,
        hosts,
    };

    Ok(Json(json!({"success": true, "data": response})))
}

/// 创建分组
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HostGroupCreateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let now = Utc::now().timestamp();
    let id = Uuid::new_v4().to_string();

    let group = HostGroupConfig {
        id: id.clone(),
        name: req.name.trim().to_string(),
        parent_id: req.parent_id.map(|id| id.to_string()),
        description: req.description,
        created_at: Some(now),
        updated_at: Some(now),
    };

    {
        let mut config = state.config.lock().await;
        config.host_groups.push(group.clone());
        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }
    }

    let response = HostGroupResponse {
        id: group.id,
        name: group.name,
        parent_id: group.parent_id,
        parent_name: None,
        description: group.description,
        host_count: 0,
        created_at: group.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        updated_at: group.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
    };

    Ok(Json(json!({"success": true, "message": "Group created", "data": response})))
}

/// 更新分组
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<HostGroupUpdateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let updated = {
        let mut config = state.config.lock().await;
        let pos = config.host_groups.iter().position(|g| g.id == id.to_string())
            .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Group not found"}))))?;

        let now = Utc::now().timestamp();
        let group = HostGroupConfig {
            id: id.to_string(),
            name: req.name.unwrap_or_else(|| config.host_groups[pos].name.clone()),
            parent_id: req.parent_id.map(|id| id.to_string()),
            description: req.description.or(config.host_groups[pos].description.clone()),
            created_at: config.host_groups[pos].created_at,
            updated_at: Some(now),
        };

        config.host_groups[pos] = group.clone();
        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }

        group
    };

    let parent_names: std::collections::HashMap<String, String> = {
        let config = state.config.lock().await;
        config.host_groups.iter()
            .filter_map(|g| Some((g.id.clone(), g.name.clone())))
            .collect()
    };

    let response = HostGroupResponse {
        id: updated.id.clone(),
        name: updated.name.clone(),
        parent_id: updated.parent_id.clone(),
        parent_name: updated.parent_id.as_ref().and_then(|id| parent_names.get(id).cloned()),
        description: updated.description.clone(),
        host_count: 0,
        created_at: updated.created_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
        updated_at: updated.updated_at.map(|ts| chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default()),
    };

    Ok(Json(json!({"success": true, "message": "Group updated", "data": response})))
}

/// 删除分组
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    {
        let mut config = state.config.lock().await;
        let pos = config.host_groups.iter().position(|g| g.id == id.to_string())
            .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Group not found"}))))?;

        // 将该分组下的主机分组 ID 设为 None
        for host in &mut config.hosts {
            if host.group_id.as_ref() == Some(&id.to_string()) {
                host.group_id = None;
            }
        }

        config.host_groups.remove(pos);
        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }
    }

    Ok((StatusCode::NO_CONTENT, Json(json!({"success": true, "message": "Group deleted"}))))
}

/// 更新分组下的主机
pub async fn update_group_hosts(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateGroupHostsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    {
        let mut config = state.config.lock().await;

        // 检查分组是否存在
        if !config.host_groups.iter().any(|g| g.id == id.to_string()) {
            return Err((StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Group not found"}))));
        }

        let target_ids: std::collections::HashSet<String> = req.host_ids.iter().map(|id| id.to_string()).collect();

        // 更新所有主机的分组
        for host in &mut config.hosts {
            if target_ids.contains(&host.id) {
                host.group_id = Some(id.to_string());
            } else if host.group_id.as_ref() == Some(&id.to_string()) {
                // 移除不再属于该分组的主机的分组标识
                host.group_id = None;
            }
        }

        if let Err(e) = crate::save_config(&config).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to save: {}", e)}))));
        }
    }

    Ok(Json(json!({"success": true, "message": "Group hosts updated"})))
}
