// 主机分组模块
// Host Group Module

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

// ============================================================================
// Request Types
// ============================================================================

/// 创建主机分组请求
#[derive(ToSchema, Deserialize, Serialize, Validate, Clone, Debug)]
pub struct HostGroupCreateRequest {
    #[schema(example = "生产环境")]
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    #[schema(example = "660e8400-e29b-41d4-a716-446655440001")]
    pub parent_id: Option<Uuid>,

    #[schema(example = "用于生产环境的主机")]
    #[validate(length(max = 500))]
    pub description: Option<String>,
}

/// 更新主机分组请求
#[derive(ToSchema, Deserialize, Serialize, Validate, Clone, Debug)]
pub struct HostGroupUpdateRequest {
    #[schema(example = "更新后的名称")]
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,

    #[schema(example = "660e8400-e29b-41d4-a716-446655440001")]
    pub parent_id: Option<Uuid>,

    #[schema(example = "更新后的描述")]
    #[validate(length(max = 500))]
    pub description: Option<String>,
}

/// 更新分组主机请求
#[derive(ToSchema, Deserialize, Serialize, Clone, Debug)]
pub struct UpdateGroupHostsRequest {
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub host_ids: Vec<Uuid>,
}

// ============================================================================
// Response Types
// ============================================================================

/// 主机分组响应
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostGroupResponse {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub parent_name: Option<String>,
    pub description: Option<String>,
    pub host_count: usize,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// 主机分组详情
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostGroupDetailResponse {
    pub group: HostGroupResponse,
    pub hosts: Vec<crate::app::hosts::models::HostResponse>,
}
