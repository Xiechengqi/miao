// 远程执行模块
// Remote Execute Module

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

// ============================================================================
// Execute Types
// ============================================================================

/// 执行命令请求
#[derive(ToSchema, Deserialize, Serialize, Validate, Clone, Debug)]
pub struct ExecuteRequest {
    #[schema(example = "df -h")]
    #[validate(length(min = 1, max = 4096))]
    pub command: String,

    #[schema(example = "-h")]
    #[validate(length(max = 32))]
    pub args: Option<Vec<String>>,

    #[schema(example = "KEY")]
    pub env: Option<std::collections::HashMap<String, String>>,

    #[schema(example = "/tmp")]
    pub working_dir: Option<String>,

    #[schema(example = 30000)]
    #[validate(range(min = 1000, max = 300000))]
    pub timeout_ms: u32,

    #[schema(example = "")]
    pub stdin: Option<String>,
}

/// 执行命令响应
#[derive(ToSchema, Serialize, Clone, Debug)]
pub struct ExecuteResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

// ============================================================================
// System Info Types
// ============================================================================

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
