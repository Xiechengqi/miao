# 订阅解析与 sing-box 版本管理

miao 使用独立的 sing-box 二进制运行代理。经过验证的版本和 Linux amd64/arm64 归档 SHA-256 固定在 `sing-box-release.json`，当前为 **1.14.4**；不会在构建或升级时自动追随 latest。

## 支持格式

解析 API、直接内容订阅、下载订阅和缓存文件共用 `src/subscription_parser.rs`：

- sing-box JSON：`outbounds` 数组。保留支持的代理出站字段；跳过 selector、urltest、direct 等管理出站。
- Clash YAML：保留既有 SS、Hysteria2、AnyTLS 支持。
- URI 列表：SS、SOCKS5、HTTP/HTTPS CONNECT、TUIC v5、Hysteria2（含 `hy2`）、VLESS。
- Base64 包装的上述内容：兼容 standard / URL-safe、有 / 无 padding 和换行。

HTTPS 同时支持 `https://user:password@host:port/#name` 和供应商的 `https://BASE64(user:password@host:port/#name)` 形式。后一种形式必须解码完整载荷，不能先按普通 URL 拆分 authority。

URI 按完整 scheme 分派，`vless://` 不会被误判成 `ss://`。凭据和名称仅做一次必要的百分号解码，保留字面 `+`。支持 IPv6、缺省节点名和重名处理，并避免覆盖 `proxy`、`_dns`、`direct` 内置出站。

## 参数与限制

| URI | sing-box 出站与参数 |
| --- | --- |
| `socks5://`、`socks://` | `socks`，`version: "5"`，可选用户名、密码 |
| `http://`、`https://` | `http`；HTTPS 使用 `tls.enabled: true` |
| `tuic://` | UUID、密码、`congestion_control`、`udp_relay_mode`、`zero_rtt_handshake`、`heartbeat`、TLS |
| `hysteria2://`、`hy2://` | 密码（保留 userpass）、TLS、`obfs`/`obfs-password`、`upmbps`/`downmbps` |
| `vless://` | UUID、TLS / Reality、uTLS 指纹、`flow`；TCP、WebSocket、gRPC transport |
| `ss://` | SIP002、完整旧式 Base64、明文百分号编码凭据；保留 obfs-local/simple-obfs/v2ray-plugin 参数 |

TLS 支持 `sni`（部分协议含 `peer`）、`alpn`、`insecure`、`allow_insecure`；默认验证证书，仅在显式指定时关闭。Hysteria2 的 `fastopen` 会产生诊断并忽略，因为 QUIC 不使用 TCP Fast Open。不会强制证书验证关闭或给 Hysteria2 设置固定带宽。

VLESS Reality 映射 `pbk`、`sid`、`fp`，缺省 short ID 为空字符串；TCP 不生成 transport 对象。未知关键参数、不支持的 transport/security、无效 UUID、端口及冲突参数会报告错误，不会静默生成不同连接方式。JSON 中的高级字段原样保留，最终仍须通过目标内核的配置检查。

Clash 的 SS 插件对象不做猜测转换，可改用 SS URI 或 sing-box JSON。解析支持与实际连通是两个检查阶段，实际连接仍取决于服务端协议、认证、TLS 和网络环境。

## API 兼容性与诊断

`POST /api/subscription/parse`：

```json
{"content": "Base64 内容、URI 列表、JSON 或 YAML"}
```

成功响应继续使用 `data: [...]` 数组，SS 的原有字段保持兼容；`message` 包含解析数量及最多 20 条诊断。有效节点会保留，错误行被跳过；全部失败时返回 `success: false` 和具体原因。

订阅文件状态新增可选 `diagnostics` 数组，每项包含 `line`、`protocol`、`message`。URI 的 line 是解码后行号，JSON/YAML 的 line 是节点序号。诊断不包含节点原始内容、地址或凭据。

`POST /api/subscriptions` 和 `PUT /api/subscriptions/{id}` 支持：

```json
{"name":"手动订阅","enabled":true,"content":"https://user:pass@proxy.example:443/#node"}
```

显式 `content` 始终作为代理内容处理，不发起订阅下载，包括无认证的单条 HTTP/HTTPS 代理链接。`url` 字段继续兼容下载地址、直接 Base64/列表内容，以及能明确识别的完整 Base64 HTTPS 分享链接。普通 HTTP/HTTPS URL（包括带 Basic Auth 的下载地址）按下载处理；普通 HTTP/HTTPS 代理链接应使用显式 `content`，避免猜测用途。

直接内容在保存前校验；下载内容在替换缓存前校验。缓存和生成配置以 0600 权限写入临时文件再原子替换。配置检查失败会保留旧配置，并阻止服务重启。

## 安装与升级

构建辅助脚本使用同一版本清单：

```bash
bash scripts/download-sing-box.sh amd64 /path/to/sing-box
```

此脚本供离线准备/构建使用，会替换目标文件；正在运行的服务应通过 miao 的安装/升级接口操作。

运行时安装与升级执行：

1. 下载固定版本，校验归档 SHA-256。
2. 仅提取预期的二进制条目，核验版本和 `with_quic`、`with_utls`、`with_clash_api` 构建标签。
3. 用新内核检查新生成的配置，校验完成前不停止旧服务。
4. 备份 `sing-box.previous` 和 `config.json.previous`，替换文件，按原状态重启。
5. 替换或启动失败时恢复旧文件；原服务在运行时尝试恢复运行。备份保留，便于处理回滚本身失败的情况。

安装、升级、配置再生成和服务控制共用操作锁，避免并发替换。`GET /api/binaries/status` 返回实际 `sing_box_home` 路径、`version_info`（版本与构建标签）和 `pinned_version`。

模板使用 route 的 sniff 动作，移除了 1.13 不支持的 inbound sniff 字段及已弃用的 DNS independent_cache。更新版本时须同步更新清单中的两个架构 SHA-256，并重新完成原生检查。

## 验证

```bash
# 后端单元、API/文件链路及解析回归测试
mkdir -p public # 若尚未构建前端，供后端测试的静态资源宏使用
cargo test --locked --all-targets --features tcp_tunnel

# 使用经过 SHA-256 校验的目标内核检查完整 TUN/DNS 配置（IPv4 与 IPv6）
MIAO_TEST_SING_BOX=/path/to/sing-box \
  cargo test --locked --features tcp_tunnel --bin miao-rust \
  native_core_validates_generated_config -- --ignored

# 可选：用本地实际订阅替代脱敏 fixture
MIAO_TEST_SING_BOX=/path/to/sing-box MIAO_TEST_SUBSCRIPTION=/tmp/sub.txt \
  cargo test --locked --features tcp_tunnel --bin miao-rust \
  native_core_validates_generated_config -- --ignored

# 只输出本地订阅的协议数量，不打印凭据
cargo run --locked --example parse_subscription -- /tmp/sub.txt
```

原生检查只运行 `sing-box check`，不启动 TUN 或连接订阅节点。仓库 fixture 是按实际格式构造的 37 条虚构节点，不包含真实订阅凭据。
