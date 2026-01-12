# Miao

一个开箱即用的 [sing-box](https://github.com/SagerNet/sing-box) 管理器。下载、配置订阅、运行，即可实现 **TUN 模式透明代理 + 国内外自动分流**。

> **当前仅支持 Hysteria2 协议节点**

## 特性

- **零配置 sing-box** - 内嵌 sing-box 二进制，无需单独安装
- **TUN 透明代理** - 系统级代理，所有流量自动走代理
- **国内外自动分流** - 基于 geosite/geoip 规则，国内直连、国外代理
- **Web 管理面板** - 节点管理、订阅文件管理、实时流量监控、测速
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

- 默认从 `./sub` 目录加载（可通过启动参数 `--sub-dir <path>` 指定）
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
sudo ./miao --sub-dir ./sub
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

> 默认规则会让所有 `tcp/22`（SSH）直连，避免代理出口对 22 端口的限制导致 SSH 断连。

## DNS 说明（DoH 优先 + 自动切换）

默认使用多个 DoH 远程 DNS（Cloudflare/Google/Quad9）并在后端定时探测可用性，必要时会自动切换 `dns.final` 并重启 sing-box 使其生效。

可在 `config.yaml` 中通过 `dns_active` / `dns_candidates` / `dns_check_interval_ms` 等字段调整策略（见 `config.yaml.example`）。
