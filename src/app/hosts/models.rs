// 主机管理模块
// Host Management Module

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::HostTestResult;

// ============================================================================
// Request Types
// ============================================================================

/// 创建主机请求
#[derive(ToSchema, Deserialize, Serialize, Validate, Clone, Debug)]
pub struct HostCreateRequest {
    #[schema(example = "测试主机")]
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,

    #[schema(example = "192.168.1.100")]
    #[validate(length(min = 1, max = 255))]
    pub host: String,

    #[schema(example = 22)]
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    #[schema(example = "root")]
    #[validate(length(min = 1, max = 64))]
    pub username: String,

    #[schema(example = "password")]
    pub auth_type: String,

    pub password: Option<String>,

    pub private_key_path: Option<String>,

    pub private_key_passphrase: Option<String>,

    pub group_id: Option<Uuid>,

    #[validate(length(max = 10))]
    pub tags: Option<Vec<String>>,

    #[schema(example = "用于测试的主机")]
    #[validate(length(max = 500))]
    pub description: Option<String>,

    #[schema(default = true)]
    pub enabled: bool,

    #[schema(default = 10000)]
    #[validate(range(min = 1000, max = 60000))]
    pub connection_timeout_ms: u32,

    #[schema(default = 30000)]
    #[validate(range(min = 1000, max = 300000))]
    pub keepalive_interval_ms: u32,
}

/// 更新主机请求
#[derive(ToSchema, Deserialize, Serialize, Validate, Clone, Debug)]
pub struct HostUpdateRequest {
    #[schema(example = "更新后的名称")]
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,

    #[schema(example = "192.168.1.100")]
    #[validate(length(min = 1, max = 255))]
    pub host: Option<String>,

    #[schema(example = 22)]
    #[validate(range(min = 1, max = 65535))]
    pub port: Option<u16>,

    #[schema(example = "root")]
    #[validate(length(min = 1, max = 64))]
    pub username: Option<String>,

    #[schema(example = "password")]
    pub auth_type: Option<String>,

    pub password: Option<String>,

    pub private_key_path: Option<String>,

    pub private_key_passphrase: Option<String>,

    pub group_id: Option<Uuid>,

    #[validate(length(max = 10))]
    pub tags: Option<Vec<String>>,

    #[schema(example = "更新后的描述")]
    #[validate(length(max = 500))]
    pub description: Option<String>,

    pub enabled: Option<bool>,

    #[validate(range(min = 1000, max = 60000))]
    pub connection_timeout_ms: Option<u32>,

    #[validate(range(min = 1000, max = 300000))]
    pub keepalive_interval_ms: Option<u32>,
}

/// 批量删除请求
#[derive(ToSchema, Deserialize, Serialize, Clone, Debug)]
pub struct BatchDeleteRequest {
    #[schema(example = "550e8400-e29b-41d4-a716-446655440000")]
    pub ids: Vec<Uuid>,
}

/// 导入主机请求
#[derive(ToSchema, Deserialize, Serialize, Clone, Debug)]
pub struct ImportHostsRequest {
    pub hosts: Vec<HostCreateRequest>,
    pub replace_existing: bool,
}

/// 列表查询参数
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct HostListParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub group_id: Option<Uuid>,
    pub search: Option<String>,
    pub tags: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// 测试主机配置请求（不需要已保存的主机）
#[derive(ToSchema, Deserialize, Serialize, Clone, Debug)]
pub struct HostTestConfigRequest {
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub private_key_passphrase: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

/// 主机响应
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostResponse {
    pub id: String,
    pub name: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key_passphrase: Option<String>,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub enabled: bool,
    pub connection_timeout_ms: u32,
    pub keepalive_interval_ms: u32,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_connected_at: Option<String>,
    pub last_test_result: Option<HostTestResult>,
}

/// 主机详情响应
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostDetailResponse {
    pub host: HostResponse,
    pub system_info: Option<HostSystemInfo>,
}

/// 分页响应
#[allow(dead_code)]
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostPageResponse {
    pub items: Vec<HostResponse>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

/// 测试响应
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostTestResponse {
    pub id: String,
    pub host: String,
    pub ssh_ok: bool,
    pub ssh_error: Option<String>,
    pub ping_avg_ms: Option<f64>,
    pub timestamp: String,
}

/// 批量测试结果
#[allow(dead_code)]
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct BatchTestResult {
    pub results: Vec<HostTestResponse>,
}

/// 导入结果
#[allow(dead_code)]
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<ImportError>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ImportError {
    pub index: usize,
    pub error: String,
}

/// 导出响应
#[allow(dead_code)]
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct ExportResponse {
    pub format: String,
    pub data: String,
    pub count: usize,
}

/// 默认私钥路径响应
#[allow(dead_code)]
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostDefaultKeyPathResponse {
    pub path: Option<String>,
}

/// 主机系统信息
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct HostSystemInfo {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub architecture: String,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disk: DiskInfo,
    pub uptime_secs: u64,
    pub load_avg: Vec<f64>,
}

#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct CpuInfo {
    pub model: String,
    pub cores: u32,
    pub frequency_mhz: Option<f64>,
}

#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
}

#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct DiskInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
}
