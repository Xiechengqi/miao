# IPv6 禁用时 Sing-box 启动失败修复方案

## 问题描述
当系统通过 `sysctl.conf` 设置 `net.ipv6.conf.all.disable_ipv6 = 1` 禁用 IPv6 后，sing-box 的 TUN 接口尝试绑定 IPv6 地址 `fd00:172:18::1/126` 时会失败，导致整个进程启动失败。

## 解决方案
采用**方案 1（动态检测）+ 方案 2（配置覆盖）**的组合方案：
- 默认自动检测系统 IPv6 状态，动态生成配置
- 允许用户通过配置文件强制控制

## 代码修改

### 1. Config 结构体 (src/main.rs:1023)
添加 IPv6 配置字段：
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
enable_ipv6_tun: Option<bool>,
```

### 2. IPv6 检测函数 (src/main.rs:10210)
```rust
fn is_ipv6_enabled() -> bool {
    std::fs::read_to_string("/proc/sys/net/ipv6/conf/all/disable_ipv6")
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .map(|v| v == 0)
        .unwrap_or(true)
}
```

### 3. 配置模板函数 (src/main.rs:10218)
修改函数签名并动态生成 TUN 地址：
```rust
fn get_config_template(enable_ipv6: bool) -> serde_json::Value {
    let tun_addresses: Vec<&str> = if enable_ipv6 {
        vec!["172.18.0.1/30", "fd00:172:18::1/126"]
    } else {
        vec!["172.18.0.1/30"]
    };
    // ... 使用 tun_addresses
}
```

### 4. gen_config 函数 (src/main.rs:10147)
调用时传递 IPv6 配置：
```rust
let enable_ipv6 = config.enable_ipv6_tun.unwrap_or_else(is_ipv6_enabled);
let mut sing_box_config = get_config_template(enable_ipv6);
```

### 5. 配置文件说明 (config.yaml.example)
更新 IPv6 说明，添加手动控制选项。

## 使用方法

### 自动模式（推荐）
无需配置，系统会自动检测 IPv6 状态：
- IPv6 启用：TUN 使用 `["172.18.0.1/30", "fd00:172:18::1/126"]`
- IPv6 禁用：TUN 使用 `["172.18.0.1/30"]`

### 手动模式
在 `config.yaml` 中添加：
```yaml
# 强制禁用 IPv6 TUN
enable_ipv6_tun: false

# 或强制启用（系统必须支持 IPv6）
enable_ipv6_tun: true
```

## 测试场景

1. **IPv6 启用环境**：保持原有行为，TUN 包含 IPv4 和 IPv6 地址
2. **IPv6 禁用环境**：自动跳过 IPv6 地址，仅使用 IPv4
3. **配置覆盖**：用户配置优先于自动检测

## 影响范围
- 最小修改：仅配置生成逻辑
- 向后兼容：IPv6 启用系统行为不变
- 无破坏性：不影响其他功能
