# iVnc 替换"桌面和应用"功能实施规划

## 一、现状分析

### 1.1 当前架构

**前端（React/Next.js）：**
- `/dashboard/apps/page.tsx` - 应用管理页面（665行）
  - 管理在 VNC 会话中运行的应用程序
  - 支持应用模板、环境变量、命令参数配置
  - 可绑定到 VNC 会话或使用独立 DISPLAY
  - 实时日志查看（WebSocket）

- `/dashboard/vnc/page.tsx` - VNC 会话管理页面（800行）
  - 基于 KasmVNC 的远程桌面管理
  - 支持分辨率、帧率、密码配置
  - 已实现安装检测（KasmVNC + i3）
  - 按系统类型显示安装指南
  - 实时日志查看（WebSocket）

**后端（Rust）：**
- `src/main.rs` 包含完整的 VNC 和 App 管理逻辑
- 核心数据结构：
  - `VncSessionConfig` / `VncSessionItem` - VNC 会话
  - `AppConfig` / `AppItem` - 应用配置
  - `AppProcess` - 进程管理
- API 端点：
  - `/api/vnc/sessions` - CRUD
  - `/api/apps` - CRUD
  - `/api/tools/status` - 检测 vncserver/i3 安装状态
  - `/api/vnc/logs/:id/ws` - 日志流
  - `/api/apps/logs/:id/ws` - 日志流

### 1.2 sing-box 安装检测模式（参考）

**前端流程：**
1. 页面加载时调用 `api.getBinariesStatus()`
2. 检查 `binStatus.sing_box.installed`
3. 未安装显示安装提示卡片
4. 点击安装按钮调用 `api.installSingBox()`
5. 升级通过 WebSocket 实时显示进度

**后端实现：**
- 检测二进制文件是否存在
- 从 GitHub releases 下载
- 设置执行权限
- 验证版本

### 1.3 iVnc 项目特性

- **独立二进制**：单文件部署，内置 Smithay Wayland 合成器
- **WebRTC 流媒体**：基于 str0m，非传统 VNC 协议
- **内置 Web UI**：自带浏览器界面，无需额外前端
- **配置文件驱动**：通过 config.toml 配置
- **HTTP API**：健康检查、Prometheus 指标
- **应用管理**：支持配置多个应用自动启动
- **下载地址**：https://github.com/Xiechengqi/iVnc/releases/download/latest/ivnc-linux-amd64

---

## 二、替换方案设计

### 2.1 核心思路

将 iVnc 作为**独立的二进制服务**集成到 miao 项目中，类似 sing-box 的集成方式：
- miao 负责 iVnc 的生命周期管理（安装、启动、停止、更新）
- miao 提供配置界面（端口、认证等）
- 用户通过 miao UI 中的"打开桌面"按钮跳转到 iVnc 的 Web 界面
- 移除现有的 apps 和 vnc 页面，统一为单一的 VNC 页面

### 2.2 架构对比

**替换前：**
```
miao 后端 → 管理 KasmVNC 进程 → 管理应用进程
miao 前端 → apps 页面 + vnc 页面
用户 → 通过 VNC 客户端连接 KasmVNC
```

**替换后：**
```
miao 后端 → 管理 iVnc 进程
miao 前端 → vnc 页面（简化）
用户 → 通过浏览器访问 iVnc 内置 Web UI
```

### 2.3 功能映射

| 现有功能 | iVnc 对应功能 | 实现方式 |
|---------|--------------|---------|
| VNC 会话管理 | iVnc 进程 | miao 管理单个 iVnc 实例 |
| 应用管理 | iVnc 应用配置 | 通过 iVnc config.toml 配置 |
| 分辨率/帧率 | iVnc 配置 | miao 生成 config.toml |
| 密码认证 | Basic Auth | miao 配置 iVnc 认证 |
| 日志查看 | iVnc 日志 | miao 读取 iVnc 日志文件 |
| 安装检测 | 二进制检测 | 检查 ~/.local/bin/ivnc |

---

## 三、详细实施计划

### 阶段 1：后端集成（Rust）

#### 1.1 数据结构定义

在 `src/main.rs` 中添加：

```rust
// iVnc 配置（存储在数据库）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IVncConfig {
    enabled: bool,
    port: u16,
    basic_auth_user: String,
    basic_auth_password: String,
    auto_start: bool,
    target_fps: u32,
    video_bitrate: u32,
}

// iVnc 状态
#[derive(Debug, Serialize)]
struct IVncStatus {
    installed: bool,
    version: Option<String>,
    running: bool,
    pid: Option<u32>,
    uptime_secs: Option<u64>,
    port: u16,
}

// iVnc 进程管理
struct IVncProcess {
    pid: u32,
    child: Child,
    started_at: Instant,
}

// 在 AppState 中添加
pub struct AppState {
    // ... 现有字段
    ivnc_process: Arc<Mutex<Option<IVncProcess>>>,
    ivnc_config: Arc<Mutex<IVncConfig>>,
}
```

#### 1.2 工具函数

```rust
// 二进制路径
fn get_ivnc_binary_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".local/bin/ivnc")
}

// 配置文件路径
fn get_ivnc_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/miao/ivnc.toml")
}

// 日志文件路径
fn get_ivnc_log_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".local/share/miao/ivnc.log")
}

// 检查安装状态
fn check_ivnc_installed() -> bool {
    get_ivnc_binary_path().exists()
}

// 获取版本
fn get_ivnc_version() -> Option<String> {
    let output = Command::new(get_ivnc_binary_path())
        .arg("--version")
        .output()
        .ok()?;

    let version_str = String::from_utf8_lossy(&output.stdout);
    // 解析版本号
    Some(version_str.trim().to_string())
}

// 生成配置文件
fn generate_ivnc_config(config: &IVncConfig) -> Result<(), String> {
    let config_path = get_ivnc_config_path();

    // 确保目录存在
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    let toml_content = format!(r#"
[http]
port = {}
basic_auth_enabled = true
basic_auth_user = "{}"
basic_auth_password = "{}"

[display]
width = 0
height = 0

[encoding]
target_fps = {}
max_fps = 60

[webrtc]
enabled = true
tcp_only = true
video_codec = "h264"
video_bitrate = {}
hardware_encoder = "auto"

[audio]
enabled = false

[logging]
level = "info"
logfile = "{}"
format = "json"
"#,
        config.port,
        config.basic_auth_user,
        config.basic_auth_password,
        config.target_fps,
        config.video_bitrate,
        get_ivnc_log_path().display()
    );

    fs::write(config_path, toml_content)
        .map_err(|e| format!("写入配置文件失败: {}", e))?;

    Ok(())
}
```

#### 1.3 API 端点实现

**扩展工具状态检测：**

```rust
// 修改现有的 get_tools_status
async fn get_tools_status() -> Json<ApiResponse<serde_json::Value>> {
    let vnc_available = check_command_exists("vncserver");
    let i3_available = check_command_exists("i3");
    let os_id = detect_os_id();

    // 新增 iVnc 检测
    let ivnc_installed = check_ivnc_installed();
    let ivnc_version = get_ivnc_version();

    Json(ApiResponse::success(json!({
        "vnc": vnc_available,
        "i3": i3_available,
        "ivnc": {
            "installed": ivnc_installed,
            "version": ivnc_version,
        },
        "os": os_id,
    })))
}
```

**安装 API：**

```rust
// POST /api/ivnc/install
async fn install_ivnc() -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    // 1. 检查是否已安装
    if check_ivnc_installed() {
        return Err((StatusCode::BAD_REQUEST, "iVnc 已安装".to_string()));
    }

    // 2. 创建目标目录
    let target_path = get_ivnc_binary_path();
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("创建目录失败: {}", e)))?;
    }

    // 3. 下载二进制
    let download_url = "https://github.com/Xiechengqi/iVnc/releases/download/latest/ivnc-linux-amd64";

    let response = reqwest::get(download_url)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("下载失败: {}", e)))?;

    if !response.status().is_success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "下载失败".to_string()));
    }

    let bytes = response.bytes()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("读取数据失败: {}", e)))?;

    // 4. 写入文件
    fs::write(&target_path, bytes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("写入文件失败: {}", e)))?;

    // 5. 设置执行权限
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("设置权限失败: {}", e)))?;
    }

    // 6. 验证安装
    if !check_ivnc_installed() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "安装验证失败".to_string()));
    }

    Ok(Json(ApiResponse::success(())))
}

// GET /api/ivnc/status
async fn get_ivnc_status(State(state): State<Arc<AppState>>) -> Json<ApiResponse<IVncStatus>> {
    let installed = check_ivnc_installed();
    let version = get_ivnc_version();
    let config = state.ivnc_config.lock().await.clone();

    let process_guard = state.ivnc_process.lock().await;
    let (running, pid, uptime_secs) = if let Some(proc) = process_guard.as_ref() {
        let uptime = proc.started_at.elapsed().as_secs();
        (true, Some(proc.pid), Some(uptime))
    } else {
        (false, None, None)
    };

    Json(ApiResponse::success(IVncStatus {
        installed,
        version,
        running,
        pid,
        uptime_secs,
        port: config.port,
    }))
}

// POST /api/ivnc/start
async fn start_ivnc(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    // 1. 检查是否已安装
    if !check_ivnc_installed() {
        return Err((StatusCode::BAD_REQUEST, "iVnc 未安装".to_string()));
    }

    // 2. 检查是否已运行
    if state.ivnc_process.lock().await.is_some() {
        return Err((StatusCode::BAD_REQUEST, "iVnc 已在运行".to_string()));
    }

    // 3. 生成配置文件
    let config = state.ivnc_config.lock().await.clone();
    generate_ivnc_config(&config)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // 4. 启动进程
    let log_path = get_ivnc_log_path();
    let child = Command::new(get_ivnc_binary_path())
        .arg("-c")
        .arg(get_ivnc_config_path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("启动失败: {}", e)))?;

    let pid = child.id();

    // 5. 保存进程信息
    state.ivnc_process.lock().await.replace(IVncProcess {
        pid,
        child,
        started_at: Instant::now(),
    });

    Ok(Json(ApiResponse::success(())))
}

// POST /api/ivnc/stop
async fn stop_ivnc(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    let mut process_guard = state.ivnc_process.lock().await;

    if let Some(mut proc) = process_guard.take() {
        // 发送 SIGTERM
        proc.child.kill()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("停止失败: {}", e)))?;

        // 等待进程退出
        let _ = proc.child.wait();
    }

    Ok(Json(ApiResponse::success(())))
}

// POST /api/ivnc/restart
async fn restart_ivnc(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    stop_ivnc(State(state.clone())).await?;
    sleep(Duration::from_secs(1)).await;
    start_ivnc(State(state)).await?;
    Ok(Json(ApiResponse::success(())))
}

// GET /api/ivnc/config
async fn get_ivnc_config(State(state): State<Arc<AppState>>) -> Json<ApiResponse<IVncConfig>> {
    let config = state.ivnc_config.lock().await.clone();
    Json(ApiResponse::success(config))
}

// PUT /api/ivnc/config
async fn update_ivnc_config(
    State(state): State<Arc<AppState>>,
    Json(new_config): Json<IVncConfig>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    // 保存配置
    *state.ivnc_config.lock().await = new_config.clone();

    // 如果正在运行，需要重启
    if state.ivnc_process.lock().await.is_some() {
        restart_ivnc(State(state)).await?;
    }

    Ok(Json(ApiResponse::success(())))
}

// GET /api/ivnc/logs
async fn get_ivnc_logs(Query(params): Query<HashMap<String, String>>) -> Result<Json<ApiResponse<Vec<LogEntry>>>, (StatusCode, String)> {
    let limit: usize = params.get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let log_path = get_ivnc_log_path();
    if !log_path.exists() {
        return Ok(Json(ApiResponse::success(vec![])));
    }

    let content = fs::read_to_string(log_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("读取日志失败: {}", e)))?;

    let logs: Vec<LogEntry> = content
        .lines()
        .rev()
        .take(limit)
        .filter_map(|line| {
            serde_json::from_str::<serde_json::Value>(line).ok().map(|v| LogEntry {
                timestamp: v["timestamp"].as_str().unwrap_or("").to_string(),
                level: v["level"].as_str().unwrap_or("info").to_string(),
                message: v["message"].as_str().unwrap_or(line).to_string(),
            })
        })
        .collect();

    Ok(Json(ApiResponse::success(logs)))
}
```

#### 1.4 路由注册

在 `main()` 函数中添加路由：

```rust
let app = Router::new()
    // ... 现有路由
    .route("/api/ivnc/install", post(install_ivnc))
    .route("/api/ivnc/status", get(get_ivnc_status))
    .route("/api/ivnc/start", post(start_ivnc))
    .route("/api/ivnc/stop", post(stop_ivnc))
    .route("/api/ivnc/restart", post(restart_ivnc))
    .route("/api/ivnc/config", get(get_ivnc_config))
    .route("/api/ivnc/config", put(update_ivnc_config))
    .route("/api/ivnc/logs", get(get_ivnc_logs))
    .with_state(state);
```

#### 1.5 数据库初始化

在数据库初始化函数中添加 iVnc 配置表：

```rust
fn init_db(conn: &Connection) -> Result<(), rusqlite::Error> {
    // ... 现有表创建

    // iVnc 配置表
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ivnc_config (
            id INTEGER PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 1,
            port INTEGER NOT NULL DEFAULT 8008,
            basic_auth_user TEXT NOT NULL DEFAULT 'admin',
            basic_auth_password TEXT NOT NULL DEFAULT 'password',
            auto_start INTEGER NOT NULL DEFAULT 0,
            target_fps INTEGER NOT NULL DEFAULT 30,
            video_bitrate INTEGER NOT NULL DEFAULT 4000
        )",
        [],
    )?;

    // 插入默认配置
    conn.execute(
        "INSERT OR IGNORE INTO ivnc_config (id, enabled, port, basic_auth_user, basic_auth_password)
         VALUES (1, 1, 8008, 'admin', 'password')",
        [],
    )?;

    Ok(())
}
```

---

### 阶段 2：前端实现（React/TypeScript）

#### 2.1 API 客户端

在 `frontend/src/lib/api.ts` 中添加：

```typescript
// iVnc 类型定义
export interface IVncStatus {
  installed: boolean;
  version: string | null;
  running: boolean;
  pid: number | null;
  uptime_secs: number | null;
  port: number;
}

export interface IVncConfig {
  enabled: boolean;
  port: number;
  basic_auth_user: string;
  basic_auth_password: string;
  auto_start: boolean;
  target_fps: number;
  video_bitrate: number;
}

// API 方法
export const api = {
  // ... 现有方法

  // iVnc 管理
  getIVncStatus: () => request<IVncStatus>("/api/ivnc/status"),
  installIVnc: () => request("/api/ivnc/install", { method: "POST" }),
  startIVnc: () => request("/api/ivnc/start", { method: "POST" }),
  stopIVnc: () => request("/api/ivnc/stop", { method: "POST" }),
  restartIVnc: () => request("/api/ivnc/restart", { method: "POST" }),
  getIVncConfig: () => request<IVncConfig>("/api/ivnc/config"),
  updateIVncConfig: (config: Partial<IVncConfig>) =>
    request("/api/ivnc/config", {
      method: "PUT",
      body: JSON.stringify(config),
    }),
  getIVncLogs: (limit = 100) =>
    request<LogEntry[]>(`/api/ivnc/logs?limit=${limit}`),
};
```

#### 2.2 新建 VNC 页面

创建 `frontend/src/app/dashboard/vnc/page.tsx`：

```typescript
"use client";

import { useEffect, useState } from "react";
import { Card, Button, Badge, Modal, Input } from "@/components/ui";
import { useStore } from "@/stores/useStore";
import { api } from "@/lib/api";
import { IVncStatus, IVncConfig, LogEntry } from "@/types/api";
import { formatUptime } from "@/lib/utils";
import {
  Monitor,
  ExternalLink,
  RefreshCw,
  Play,
  Square,
  Settings,
  Download,
  AlertTriangle,
  FileText,
} from "lucide-react";

export default function VncPage() {
  const { setLoading, loading, addToast } = useStore();

  const [status, setStatus] = useState<IVncStatus | null>(null);
  const [config, setConfig] = useState<IVncConfig | null>(null);
  const [installing, setInstalling] = useState(false);
  const [showConfigModal, setShowConfigModal] = useState(false);
  const [showLogsModal, setShowLogsModal] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);

  useEffect(() => {
    loadStatus();
    loadConfig();
  }, []);

  const loadStatus = async () => {
    try {
      const data = await api.getIVncStatus();
      setStatus(data);
    } catch (error) {
      console.error("Failed to load iVnc status:", error);
    }
  };

  const loadConfig = async () => {
    try {
      const data = await api.getIVncConfig();
      setConfig(data);
    } catch (error) {
      console.error("Failed to load iVnc config:", error);
    }
  };

  const loadLogs = async () => {
    try {
      const data = await api.getIVncLogs(200);
      setLogs(data);
    } catch (error) {
      console.error("Failed to load logs:", error);
    }
  };

  const handleInstall = async () => {
    setInstalling(true);
    try {
      await api.installIVnc();
      addToast({ type: "success", message: "iVnc 安装成功" });
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "安装失败",
      });
    } finally {
      setInstalling(false);
    }
  };

  const handleStart = async () => {
    setLoading(true, "start");
    try {
      await api.startIVnc();
      addToast({ type: "success", message: "iVnc 已启动" });
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "启动失败",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    setLoading(true, "stop");
    try {
      await api.stopIVnc();
      addToast({ type: "success", message: "iVnc 已停止" });
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "停止失败",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleRestart = async () => {
    setLoading(true, "restart");
    try {
      await api.restartIVnc();
      addToast({ type: "success", message: "iVnc 已重启" });
      await loadStatus();
    } catch (error) {
      addToast({
        type: "error",
        message: error instanceof Error ? error.message : "重启失败",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleOpenDesktop = () => {
    if (!status || !status.running) return;
    const url = `http://${window.location.hostname}:${status.port}`;
    window.open(url, "_blank");
  };

  // 未安装提示
  if (status?.installed === false) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-black">远程桌面</h1>
          <p className="text-slate-500 mt-1">基于 iVnc 的 WebRTC 桌面流媒体</p>
        </div>
        <Card className="p-6 bg-amber-50 border-amber-200">
          <div className="flex items-start gap-3">
            <AlertTriangle className="w-5 h-5 text-amber-600 shrink-0 mt-0.5" />
            <div className="space-y-3 w-full">
              <p className="font-semibold text-amber-800">iVnc 未安装</p>
              <p className="text-sm text-amber-700">
                iVnc 是基于 Wayland 的高性能桌面流媒体服务，使用 WebRTC 实现低延迟传输。
              </p>
              <Button onClick={handleInstall} loading={installing}>
                <Download className="w-4 h-4" />
                安装 iVnc
              </Button>
            </div>
          </div>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-black">远程桌面</h1>
          <p className="text-slate-500 mt-1">基于 iVnc 的 WebRTC 桌面流媒体</p>
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" onClick={() => { loadLogs(); setShowLogsModal(true); }}>
            <FileText className="w-4 h-4" />
            日志
          </Button>
          <Button variant="secondary" onClick={() => setShowConfigModal(true)}>
            <Settings className="w-4 h-4" />
            配置
          </Button>
        </div>
      </div>

      {/* Status Card */}
      <Card className="p-6">
        <div className="flex items-center justify-between">
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 rounded-lg bg-violet-600/10 flex items-center justify-center">
              <Monitor className="w-6 h-6 text-violet-600" />
            </div>
            <div>
              <div className="flex items-center gap-2 mb-1">
                <span className="text-xl font-bold">iVnc 桌面</span>
                <Badge variant={status?.running ? "success" : "default"}>
                  {status?.running ? "运行中" : "已停止"}
                </Badge>
              </div>
              <p className="text-sm text-slate-500">
                版本: {status?.version || "未知"} · 端口: {status?.port}
              </p>
              {status?.running && (
                <p className="text-sm text-slate-500">
                  PID: {status.pid} · 运行时间: {formatUptime(status.uptime_secs || 0)}
                </p>
              )}
            </div>
          </div>

          <div className="flex gap-2">
            {status?.running ? (
              <>
                <Button variant="primary" onClick={handleOpenDesktop}>
                  <ExternalLink className="w-4 h-4" />
                  打开桌面
                </Button>
                <Button variant="secondary" onClick={handleRestart}>
                  <RefreshCw className="w-4 h-4" />
                  重启
                </Button>
                <Button variant="secondary" onClick={handleStop}>
                  <Square className="w-4 h-4" />
                  停止
                </Button>
              </>
            ) : (
              <Button onClick={handleStart}>
                <Play className="w-4 h-4" />
                启动
              </Button>
            )}
          </div>
        </div>
      </Card>

      {/* 配置 Modal */}
      {/* 日志 Modal */}
      {/* ... 省略 Modal 实现 */}
    </div>
  );
}
```

#### 2.3 导航菜单调整

修改 `frontend/src/app/dashboard/layout.tsx`：

```typescript
// 移除或注释掉 apps 菜单项
const menuItems = [
  { name: "概览", path: "/dashboard", icon: Home },
  { name: "代理", path: "/dashboard/proxies", icon: Zap },
  { name: "隧道", path: "/dashboard/tunnels", icon: Network },
  { name: "远程桌面", path: "/dashboard/vnc", icon: Monitor }, // 保留，指向新的 VNC 页面
  // { name: "应用", path: "/dashboard/apps", icon: AppWindow }, // 删除
  { name: "终端", path: "/dashboard/terminals", icon: Terminal },
  { name: "主机", path: "/dashboard/hosts", icon: Server },
  { name: "同步", path: "/dashboard/sync", icon: RefreshCw },
  { name: "日志", path: "/dashboard/logs", icon: FileText },
  { name: "关于", path: "/dashboard/about", icon: Info },
];
```

#### 2.4 类型定义

在 `frontend/src/types/api.ts` 中添加：

```typescript
export interface IVncStatus {
  installed: boolean;
  version: string | null;
  running: boolean;
  pid: number | null;
  uptime_secs: number | null;
  port: number;
}

export interface IVncConfig {
  enabled: boolean;
  port: number;
  basic_auth_user: string;
  basic_auth_password: string;
  auto_start: boolean;
  target_fps: number;
  video_bitrate: number;
}
```

---

### 阶段 3：清理和迁移

#### 3.1 删除旧代码

**前端：**
```bash
# 删除应用管理页面
rm frontend/src/app/dashboard/apps/page.tsx

# 备份旧的 VNC 页面（可选）
mv frontend/src/app/dashboard/vnc/page.tsx frontend/src/app/dashboard/vnc/page.tsx.old

# 删除相关 mock 数据
# 编辑 frontend/src/mocks/data/apps.ts - 可以删除
# 编辑 frontend/src/mocks/data/vnc.ts - 需要更新为 iVnc 格式
```

**后端：**
```rust
// 在 src/main.rs 中注释或删除以下内容：
// - VncSessionConfig, VncSessionItem 相关结构体
// - AppConfig, AppItem, AppProcess 相关结构体
// - get_vnc_sessions, create_vnc_session 等 VNC API
// - get_apps, create_app 等 App API
// - VNC 和 App 进程管理逻辑

// 保留：
// - get_tools_status (需要扩展支持 iVnc)
```

#### 3.2 数据库迁移

如果需要保留用户数据，可以考虑：

```sql
-- 备份旧数据（可选）
CREATE TABLE vnc_sessions_backup AS SELECT * FROM vnc_sessions;
CREATE TABLE apps_backup AS SELECT * FROM apps;

-- 删除旧表
DROP TABLE IF EXISTS vnc_sessions;
DROP TABLE IF EXISTS apps;
DROP TABLE IF EXISTS app_templates;

-- 创建新表在 init_db 中已定义
```

#### 3.3 配置文件更新

在 `config.yaml.example` 中添加 iVnc 配置示例：

```yaml
# iVnc 远程桌面配置
ivnc:
  enabled: true
  port: 8008
  basic_auth_user: admin
  basic_auth_password: password
  auto_start: false
  target_fps: 30
  video_bitrate: 4000
```

---

### 阶段 4：测试计划

#### 4.1 功能测试

**安装流程：**
1. ✅ 访问 VNC 页面，显示"未安装"提示
2. ✅ 点击"安装 iVnc"按钮
3. ✅ 等待下载和安装完成
4. ✅ 验证安装状态更新为"已安装"

**启动/停止：**
1. ✅ 点击"启动"按钮，iVnc 进程启动
2. ✅ 状态显示为"运行中"，显示 PID 和运行时间
3. ✅ 点击"打开桌面"，新标签页打开 iVnc Web UI
4. ✅ 点击"停止"按钮，进程正常退出
5. ✅ 点击"重启"按钮，进程重启成功

**配置管理：**
1. ✅ 修改端口号，保存后重启生效
2. ✅ 修改认证信息，访问 iVnc 需要新密码
3. ✅ 修改帧率和码率，视频质量变化

**日志查看：**
1. ✅ 点击"日志"按钮，显示 iVnc 日志
2. ✅ 日志按时间倒序显示
3. ✅ 日志包含启动、连接、错误等信息

#### 4.2 兼容性测试

- ✅ 不同浏览器（Chrome, Firefox, Safari）
- ✅ 不同操作系统（Ubuntu, Debian, CentOS）
- ✅ 不同架构（x86_64, aarch64）

#### 4.3 性能测试

- ✅ iVnc 启动时间 < 3秒
- ✅ WebRTC 连接建立时间 < 2秒
- ✅ 视频延迟 < 100ms
- ✅ CPU 占用 < 30%（空闲时）

---

### 阶段 5：部署和回滚

#### 5.1 部署步骤

```bash
# 1. 备份当前版本
cp -r /data/projects/miao /data/projects/miao.backup

# 2. 拉取新代码
cd /data/projects/miao
git pull

# 3. 编译后端
bash build.sh

# 4. 编译前端
cd frontend
npm install
npm run build
cd ..

# 5. 重启服务
bash run.sh
```

#### 5.2 回滚方案

如果出现问题，可以快速回滚：

```bash
# 停止服务
pkill -f miao

# 恢复备份
rm -rf /data/projects/miao
mv /data/projects/miao.backup /data/projects/miao

# 重启服务
cd /data/projects/miao
bash run.sh
```

---

## 四、实施优先级和时间估算

### 优先级排序

**P0 - 核心功能（必须）：**
1. 后端 iVnc 二进制管理（安装、启动、停止）
2. 前端 VNC 页面基础 UI
3. 安装检测和自动安装
4. 删除旧的 apps 和 vnc 代码

**P1 - 重要功能（应该）：**
1. 配置管理（端口、认证、帧率）
2. 日志查看
3. 状态监控（运行时间、PID）
4. 重启功能

**P2 - 增强功能（可选）：**
1. 自动更新检测
2. 性能监控（CPU、内存）
3. 多实例支持
4. 健康检查

### 时间估算

- **后端开发**：4-6 小时
  - 数据结构和工具函数：1h
  - API 端点实现：2h
  - 路由和数据库：1h
  - 测试和调试：1-2h

- **前端开发**：3-4 小时
  - API 客户端：0.5h
  - VNC 页面 UI：1.5h
  - 配置和日志 Modal：1h
  - 测试和调试：1h

- **清理和迁移**：1-2 小时
  - 删除旧代码：0.5h
  - 数据库迁移：0.5h
  - 文档更新：0.5h

- **测试和部署**：2-3 小时
  - 功能测试：1h
  - 兼容性测试：0.5h
  - 部署和验证：0.5-1h

**总计：10-15 小时**

---

## 五、风险和注意事项

### 5.1 技术风险

**风险 1：iVnc 二进制下载失败**
- **影响**：用户无法安装 iVnc
- **缓解**：提供备用下载源，或支持手动上传二进制文件
- **应对**：显示详细错误信息，提供手动安装指南

**风险 2：iVnc 进程启动失败**
- **影响**：用户无法使用远程桌面
- **缓解**：检查依赖库（GStreamer, PulseAudio）
- **应对**：在日志中显示详细错误，提供依赖安装指南

**风险 3：端口冲突**
- **影响**：iVnc 无法启动
- **缓解**：默认使用非常用端口（8008），支持自定义端口
- **应对**：启动前检查端口占用，提示用户修改配置

**风险 4：WebRTC 连接失败**
- **影响**：用户无法访问桌面
- **缓解**：使用 TCP-only 模式，避免 NAT 穿透问题
- **应对**：提供连接诊断工具，检查防火墙和网络配置

### 5.2 用户体验风险

**风险 1：功能缺失**
- **影响**：用户习惯的应用管理功能消失
- **缓解**：在文档中说明 iVnc 的应用配置方式
- **应对**：提供迁移指南，说明如何在 iVnc 中配置应用

**风险 2：学习成本**
- **影响**：用户需要重新学习 iVnc 的使用方式
- **缓解**：提供详细的使用文档和视频教程
- **应对**：在 UI 中添加帮助提示和快速入门指南

### 5.3 兼容性风险

**风险 1：系统依赖缺失**
- **影响**：iVnc 无法运行
- **缓解**：在安装时检测依赖，提供安装指南
- **应对**：显示缺失的依赖列表，提供一键安装脚本

**风险 2：架构不支持**
- **影响**：非 x86_64 系统无法使用
- **缓解**：提供多架构二进制（aarch64）
- **应对**：检测架构，提示用户从源码编译

---

## 六、总结

### 6.1 替换方案优势

1. **简化架构**：从"miao 管理 KasmVNC + 应用"简化为"miao 管理 iVnc"
2. **更好的性能**：WebRTC 低延迟，硬件加速支持
3. **更少的依赖**：无需安装 KasmVNC、i3 等外部工具
4. **统一管理**：应用配置集成在 iVnc 中，无需单独管理
5. **现代化体验**：内置 Web UI，支持 PWA

### 6.2 关键实施要点

1. **参考 sing-box 模式**：安装检测、自动下载、进程管理
2. **保持简洁**：UI 只提供启动/停止/配置，详细功能在 iVnc Web UI 中
3. **渐进式迁移**：先实现核心功能，再逐步完善
4. **充分测试**：确保安装、启动、连接流程稳定可靠
5. **提供文档**：帮助用户理解新的使用方式

### 6.3 后续优化方向

1. **自动更新**：定期检查 iVnc 新版本，一键升级
2. **性能监控**：集成 Prometheus 指标，显示 CPU/内存/带宽
3. **多实例支持**：允许运行多个 iVnc 实例（不同端口）
4. **应用市场**：提供常用应用的配置模板（Chrome、VSCode 等）
5. **录制回放**：支持桌面会话录制和回放

---

## 附录

### A. 相关文件清单

**需要修改的文件：**
- `src/main.rs` - 添加 iVnc 管理逻辑
- `frontend/src/lib/api.ts` - 添加 iVnc API 客户端
- `frontend/src/types/api.ts` - 添加类型定义
- `frontend/src/app/dashboard/vnc/page.tsx` - 重写 VNC 页面
- `frontend/src/app/dashboard/layout.tsx` - 更新导航菜单
- `config.yaml.example` - 添加 iVnc 配置示例

**需要删除的文件：**
- `frontend/src/app/dashboard/apps/page.tsx`
- `frontend/src/mocks/data/apps.ts`（可选）

**需要备份的文件：**
- `frontend/src/app/dashboard/vnc/page.tsx` → `vnc/page.tsx.old`

### B. API 端点对照表

| 功能 | 旧 API | 新 API |
|------|--------|--------|
| 检查安装状态 | `/api/tools/status` | `/api/tools/status` (扩展) |
| 安装 | - | `/api/ivnc/install` |
| 获取状态 | `/api/vnc/sessions` | `/api/ivnc/status` |
| 启动 | `/api/vnc/sessions/:id/start` | `/api/ivnc/start` |
| 停止 | `/api/vnc/sessions/:id/stop` | `/api/ivnc/stop` |
| 重启 | `/api/vnc/sessions/:id/restart` | `/api/ivnc/restart` |
| 获取配置 | - | `/api/ivnc/config` |
| 更新配置 | `/api/vnc/sessions/:id` | `/api/ivnc/config` |
| 获取日志 | `/api/vnc/logs/:id` | `/api/ivnc/logs` |
| 应用管理 | `/api/apps/*` | 删除（由 iVnc 管理） |

### C. 依赖库清单

**iVnc 运行时依赖：**
```bash
# Ubuntu/Debian
apt-get install \
  libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 \
  libpixman-1-0 libxkbcommon0 \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
  libpulse0 libopus0 pulseaudio

# 可选：硬件加速
apt-get install gstreamer1.0-vaapi  # Intel VA-API
```

**miao 新增依赖（Cargo.toml）：**
```toml
[dependencies]
reqwest = { version = "0.11", features = ["json"] }  # 用于下载 iVnc 二进制
```

---

**规划文档版本：** v1.0
**创建日期：** 2026-03-16
**预计实施周期：** 2-3 天
**风险等级：** 中等（有回滚方案）

```

