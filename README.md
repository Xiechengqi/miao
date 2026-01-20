# Miao

一个开箱即用的 [sing-box](https://github.com/SagerNet/sing-box) 管理器。下载、配置订阅、运行，即可实现 **TUN 模式透明代理 + 国内外自动分流**。

> **当前仅支持 Hysteria2 协议节点**

## 特性

- **零配置 sing-box** - 内嵌 sing-box 二进制，无需单独安装
- **TUN 透明代理** - 系统级代理，所有流量自动走代理
- **国内外自动分流** - 基于 geosite/geoip 规则，国内直连、国外代理
- **Web 管理面板** - 节点管理、订阅文件管理、实时流量监控、测速
- **TCP 穿透 (SSH -R)** - 将本机 TCP 端口映射到远程服务器端口（可选特性 `tcp_tunnel`）
- **Web Terminal** - 内置 gotty 提供浏览器终端（可选）
- **KasmVNC 桌面访问** - 通过 VNC Web 端访问桌面/应用（可选）
- **桌面应用管理** - Chromium/CCSwitch 等应用可在 Web 面板启动并绑定到 VNC DISPLAY
- **Sync 同步** - 基于 sy 的本地文件/目录同步到 SSH 远端（支持定时 cron）
- **节点候选池 + 自动切换** - 支持 Ctrl/⌘ 多选候选节点，后端定时健康检查失败自动切换
- **自动更新** - 支持从 GitHub 一键更新到最新版本
- **OpenWrt 支持** - 自动安装所需内核模块

<img width="2560" height="1440" alt="image" src="https://github.com/user-attachments/assets/e5e101c1-6002-423b-956a-e4730c67bc12" />

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

订阅文件：

- 默认从 `./sub` 目录加载（可通过启动参数 `--sub <path>` 指定）
- `--sub <git_url>` 支持从 Git 仓库克隆到 `./sub`（启动时 clone；面板“重载”会先 `git pull` 同步一次）
- 目录下的普通文件会按文件名字典序加载，重复条目按“后加载覆盖前加载”
- 文件格式与之前的订阅链接解析格式一致（sing-box JSON / Clash YAML / SS URL 列表等）；解析失败会跳过该文件

手动配置节点：

```yaml
# 手动配置节点
nodes:
  - '{"type":"hysteria2","tag":"节点名","server":"example.com","server_port":443,"password":"xxx"}'
  # server 是 IP 时需指定 sni
  - '{"type":"hysteria2","tag":"节点2","server":"1.2.3.4","server_port":443,"password":"xxx","sni":"example.com"}'
```

### 3. 运行

```bash
sudo ./miao --sub ./sub
```

访问 `http://localhost:6161` 打开管理面板。

> 如果当前目录没有 `config.yaml`，首次打开页面会进入“初始化设置”，填写登录密码后会自动生成 `config.yaml` 并启动 Web 服务。

## 配置说明

| 字段 | 说明 | 默认值 |
|------|------|--------|
| `port` | Web 面板端口 | `6161` |
| `password` | Web 登录密码 | `admin` |
| `selections` | 记住的节点选择（selector -> node） | `{}` |
| `proxy_pool` | 代理候选节点池（按顺序优先级，2+ 时启用自动切换） | - |
| `nodes` | 手动配置的节点 (JSON 格式) | - |
| `terminals` | Web Terminal (gotty) 配置列表 | - |
| `vnc_sessions` | KasmVNC 会话配置列表 | - |
| `apps` | 桌面应用配置列表 | - |
| `syncs` | sy 文件同步配置列表 | - |

### Web Terminal (gotty)

启用后会在独立端口启动 gotty（默认 `127.0.0.1:7681`），登录认证由 miao 配置。可在 `config.yaml` 中配置多个终端节点，支持独立端口、地址、命令和额外参数（见 `config.yaml.example`）；面板里留空认证不会清空，需勾选“清除认证”。默认额外参数为 `-w --enable-idle-alert`。

> 默认规则会让所有 `tcp/22`（SSH）直连，避免代理出口对 22 端口的限制导致 SSH 断连。

### KasmVNC Sessions

启用后会在独立端口启动 KasmVNC（默认 `0.0.0.0:7900`）。每个会话可指定 `DISPLAY`、分辨率、帧率、密码等，访问地址为 `http://<host>:<port>`（不使用 https）。

### Desktop Apps

桌面应用支持绑定到某个 VNC 会话（自动使用该 DISPLAY），或手动指定 DISPLAY。应用模板预设可快速生成 Chromium/CCSwitch 等配置，后续可在面板中编辑参数与环境变量。

### Sync (sy)

Sync 功能使用 sy 在本地和 SSH 远端同步文件/目录，默认只新增或更新（不删除远端多余文件）。

注意事项：

- 远端必须预先安装 sy（包含 sy-remote）
- 仅支持 SSH 用户名/密码认证（密码留空时使用本机 ~/.ssh 下的密钥）
- 多路径同步时远端路径固定为本地路径
- 定时执行使用 cron 表达式，默认时区为 `Asia/Shanghai`
- 同步时不校验 SSH 主机指纹

## DNS 说明（DoH 优先 + 自动切换）

默认使用多个 DoH 远程 DNS（Cloudflare/Google/Quad9）并在后端定时探测可用性。

可在 `config.yaml` 中通过 `dns_active` / `dns_candidates` / `dns_check_interval_ms` 等字段调整策略（见 `config.yaml.example`）。

如果希望“切换 DNS 不重启 sing-box”，需要把 DNS 切换从 sing-box 配置层挪到外部（例如让 sing-box 只指向本地 `smartdns/mosdns/dnsmasq`，由本地转发器做上游健康检查与切换）。
