# Miao

一个基于 SSH 协议的服务器运维管理工具，提供 Web 管理面板，集成 SSH 隧道、远程终端、文件同步、VNC 桌面等功能。

## 特性

- **SSH 主机管理** - 集中管理多台 SSH 服务器，支持密码/私钥认证
- **TCP 穿透 (SSH -R)** - 基于 SSH 反向隧道，将本机端口映射到远程服务器
- **Web Terminal** - 内置 gotty 提供浏览器终端，无需本地 SSH 客户端
- **文件同步 (sy)** - 基于 SSH 的本地文件/目录同步到远端（支持定时 cron）
- **KasmVNC 桌面** - 通过 VNC Web 端访问远程桌面/应用
- **桌面应用管理** - Chromium 等应用可在 Web 面板启动并绑定到 VNC DISPLAY
- **流量转发** - 集成 sing-box，支持通过 SSH 隧道转发流量
- **自动更新** - 支持从 GitHub 一键更新到最新版本

## 技术栈

| 层级 | 技术 |
|------|------|
| 后端 | Rust + axum + tokio + rusqlite |
| 前端 | Next.js 16 + React 19 + TypeScript 5 + Tailwind CSS 4 |
| 状态管理 | Zustand |
| 动画 | Framer Motion |
| 流量转发 | sing-box（内嵌） |

## 目录结构

```
miao/
├── src/                    # Rust 后端源码
│   ├── main.rs             # 主程序入口（API 路由、业务逻辑）
│   ├── tcp_tunnel.rs       # SSH 反向隧道管理
│   ├── full_tunnel.rs      # 全隧道配置
│   └── sync.rs             # 文件同步模块
├── frontend/               # Next.js 前端
│   └── src/
│       ├── app/            # 页面组件 (App Router)
│       │   ├── dashboard/  # 管理面板各页面
│       │   ├── login/      # 登录页
│       │   └── setup/      # 初始化页
│       ├── components/ui/  # Claymorphism UI 组件库
│       ├── stores/         # Zustand 状态管理
│       ├── hooks/          # 自定义 Hooks
│       └── types/          # TypeScript 类型定义
├── embedded/               # 内嵌二进制文件
│   ├── sing-box-amd64/arm64
│   ├── gotty-amd64/arm64
│   └── sy-amd64/arm64
├── public/                 # 静态资源（Next.js 构建产物）
├── config.yaml             # 运行时配置
├── build.sh                # 本地构建脚本
├── build-ci.sh             # CI 交叉编译脚本
├── run.sh                  # 运行脚本
└── ui-test.sh              # UI 自动化测试脚本
```

## 功能模块

### SSH 主机管理

| 功能 | 说明 |
|------|------|
| 主机列表 | 集中管理多台 SSH 服务器 |
| 认证方式 | 支持密码、私钥、SSH Agent |
| 分组管理 | 按项目/环境分组管理主机 |
| 连接测试 | 一键测试 SSH 连接状态 |

### TCP 隧道 (SSH -R)

- 基于 SSH `-R` 反向隧道实现端口映射
- 支持密码/私钥/SSH Agent 认证
- 端口自动扫描（tcp_tunnel_sets）
- 连接状态：`Stopped` → `Connecting` → `Forwarding` → `Error`
- 断线重连（指数退避算法）

### 文件同步 (sy)

- 基于 SSH 协议的本地文件/目录同步到远端
- 支持 cron 定时任务
- 同步选项：压缩、限速、排除规则、删除同步
- 验证模式确保数据完整性

### 流量转发 (sing-box)

集成 sing-box 提供流量转发功能，可配合 SSH 隧道使用：

| 功能 | 说明 |
|------|------|
| TUN 模式 | 系统级流量拦截 |
| 分流规则 | 基于 geosite/geoip 规则分流 |
| DNS 健康检查 | 多 DoH 服务器自动探测与切换 |

### Web 管理面板

| 页面 | 功能 |
|------|------|
| `/dashboard` | 仪表盘、服务状态 |
| `/dashboard/hosts` | SSH 主机管理 |
| `/dashboard/tunnels` | TCP 隧道配置与管理 |
| `/dashboard/sync` | 文件同步配置、定时任务 |
| `/dashboard/terminals` | Web Terminal (gotty) 配置 |
| `/dashboard/vnc` | KasmVNC 远程桌面配置 |
| `/dashboard/apps` | 桌面应用管理（绑定 VNC） |
| `/dashboard/proxies` | 流量转发节点管理 |
| `/dashboard/logs` | 实时日志（WebSocket 流） |

## 快速开始

### 1. 下载

```bash
mkdir ~/miao && cd ~/miao

# Linux amd64
wget https://github.com/YUxiangLuo/miao/releases/latest/download/miao-rust-linux-amd64 -O miao && chmod +x miao

# Linux arm64 (树莓派、路由器等)
wget https://github.com/YUxiangLuo/miao/releases/latest/download/miao-rust-linux-arm64 -O miao && chmod +x miao
```

### 2. 配置

在同一目录下创建 `config.yaml`：

```yaml
# Web 登录密码（可选，默认 admin）
password: admin
```

### 3. 运行

```bash
sudo ./miao
```

访问 `http://localhost:6161` 打开管理面板。

> 如果当前目录没有 `config.yaml`，首次打开页面会进入"初始化设置"，填写登录密码后会自动生成 `config.yaml` 并启动 Web 服务。

## 配置说明

| 字段 | 说明 | 默认值 |
|------|------|--------|
| `port` | Web 面板端口 | `6161` |
| `password` | Web 登录密码 | `admin` |
| `hosts` | SSH 主机配置列表 | - |
| `host_groups` | 主机分组配置 | - |
| `tcp_tunnels` | SSH 隧道配置列表 | - |
| `terminals` | Web Terminal (gotty) 配置列表 | - |
| `vnc_sessions` | KasmVNC 会话配置列表 | - |
| `apps` | 桌面应用配置列表 | - |
| `syncs` | sy 文件同步配置列表 | - |

### Web Terminal (gotty)

启用后会在独立端口启动 gotty（默认 `127.0.0.1:7681`），登录认证由 miao 配置。可在 `config.yaml` 中配置多个终端节点，支持独立端口、地址、命令和额外参数；面板里留空认证不会清空，需勾选"清除认证"。默认额外参数为 `-w --enable-idle-alert`。

### KasmVNC Sessions

启用后会在独立端口启动 KasmVNC（默认 `0.0.0.0:7900`）。每个会话可指定 `DISPLAY`、分辨率、帧率、密码等，访问地址为 `http://<host>:<port>`（不使用 https）。

### Desktop Apps

桌面应用支持绑定到某个 VNC 会话（自动使用该 DISPLAY），或手动指定 DISPLAY。应用模板预设可快速生成 Chromium 等配置，后续可在面板中编辑参数与环境变量。

### Sync (sy)

Sync 功能使用 sy 在本地和 SSH 远端同步文件/目录，默认只新增或更新（不删除远端多余文件）。

注意事项：

- 远端必须预先安装 sy（包含 sy-remote）
- 仅支持 SSH 用户名/密码认证（密码留空时使用本机 ~/.ssh 下的密钥）
- 多路径同步时远端路径固定为本地路径
- 定时执行使用 cron 表达式，默认时区为 `Asia/Shanghai`
- 同步时不校验 SSH 主机指纹

## 本地开发

### 环境要求

- Rust 1.88+
- Node.js 20+
- Go（用于交叉编译）

### 构建

```bash
# 构建前端 + 后端
bash ./build.sh

# 运行
bash ./run.sh

# UI 自动化测试
bash ./ui-test.sh

# CI 交叉编译
bash ./build-ci.sh amd64   # AMD64
bash ./build-ci.sh arm64   # ARM64
```

### 前端开发

```bash
cd frontend

# Mock 模式（无需后端）
npm run dev:mock

# 真实 API 模式
npm run dev:real
```

## 免责声明

本项目仅供学习和研究目的，禁止用于商业用途。使用者应遵守当地法律法规，因使用本项目产生的任何问题由使用者自行承担。

## 协议

MIT License
