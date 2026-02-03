use axum::{
    extract::{ws::{WebSocketUpgrade}, Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::app::AppState;

use super::models::*;

// ============================================================================
// API Handlers
// ============================================================================

/// 执行命令
pub async fn execute_command(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 检查主机是否存在
    {
        let config = state.config.lock().await;
        if !config.hosts.iter().any(|h| h.id == id.to_string()) {
            return Err((StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Host not found"}))));
        }
    }

    let start = std::time::Instant::now();

    // 构建命令
    let mut cmd = tokio::process::Command::new(&req.command);

    // 添加参数
    if let Some(args) = &req.args {
        cmd.args(args);
    }

    // 添加环境变量
    if let Some(env) = &req.env {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }

    // 设置工作目录
    if let Some(dir) = &req.working_dir {
        cmd.current_dir(dir);
    }

    // 设置超时
    let timeout_duration = Duration::from_millis(req.timeout_ms as u64);

    // 添加 stdin
    if let Some(_stdin) = &req.stdin {
        cmd.stdin(std::process::Stdio::piped());
    }

    // 捕获输出
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // 启动进程
    let mut child = cmd.spawn()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to spawn command: {}", e)}))))?;

    // 写入 stdin
    if let Some(stdin) = &req.stdin {
        if let Some(ref mut child_stdin) = child.stdin {
            use tokio::io::AsyncWriteExt;
            child_stdin.write_all(stdin.as_bytes()).await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to write stdin: {}", e)}))))?;
            child_stdin.shutdown().await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to close stdin: {}", e)}))))?;
        }
    }

    // 等待完成（带超时）
    let output = tokio::time::timeout(timeout_duration, child.wait_with_output())
        .await
        .map_err(|_| (StatusCode::REQUEST_TIMEOUT, Json(json!({"success": false, "error": "Command timeout"}))))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"success": false, "error": format!("Failed to get output: {}", e)}))))?;

    let duration_ms = start.elapsed().as_millis() as u64;

    let response = ExecuteResponse {
        exit_code: output.status.code().unwrap_or(-1) as i32,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration_ms,
    };

    Ok(Json(json!({"success": true, "data": response})))
}

/// 获取主机系统信息
pub async fn get_host_info(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 检查主机是否存在
    {
        let config = state.config.lock().await;
        if !config.hosts.iter().any(|h| h.id == id.to_string()) {
            return Err((StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Host not found"}))));
        }
    }

    // 返回模拟的系统信息（实际需要通过 SSH 连接获取）
    let info = HostSystemInfo {
        hostname: "unknown".to_string(),
        os: "Linux".to_string(),
        kernel: "5.x".to_string(),
        architecture: "x86_64".to_string(),
        cpu: CpuInfo {
            model: "Unknown".to_string(),
            cores: 4,
            frequency_mhz: None,
        },
        memory: MemoryInfo {
            total_bytes: 0,
            used_bytes: 0,
            used_percent: 0.0,
        },
        disk: DiskInfo {
            total_bytes: 0,
            used_bytes: 0,
            used_percent: 0.0,
        },
        uptime_secs: 0,
        load_avg: vec![0.0, 0.0, 0.0],
    };

    Ok(Json(json!({"success": true, "data": info})))
}

/// WebSocket Shell 连接
pub async fn shell_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _ws: WebSocketUpgrade,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 检查主机是否存在
    {
        let config = state.config.lock().await;
        if !config.hosts.iter().any(|h| h.id == id.to_string()) {
            return Err((StatusCode::NOT_FOUND, Json(json!({"success": false, "error": "Host not found"}))));
        }
    }

    // 返回一个占位响应 - WebSocket 需要特殊处理
    Ok(Json(json!({
        "success": true,
        "message": "WebSocket endpoint - use ws://host/api/v1/hosts/{id}/shell",
        "note": "Actual WebSocket implementation needs separate route"
    })))
}
