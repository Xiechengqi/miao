//! Subscription decoding and URI conversion. Diagnostics never contain source credentials.
use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use url::Url;

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub line: usize,
    pub protocol: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ParseReport {
    pub outbounds: Vec<Value>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    for engine in [
        general_purpose::STANDARD,
        general_purpose::STANDARD_NO_PAD,
        general_purpose::URL_SAFE,
        general_purpose::URL_SAFE_NO_PAD,
    ] {
        if let Ok(bytes) = engine.decode(&cleaned) {
            return Ok(bytes);
        }
    }
    Err("Invalid Base64 encoding".into())
}

fn decoded(input: &str) -> Result<String, String> {
    String::from_utf8(decode_base64(input)?).map_err(|_| "Base64 content is not UTF-8".into())
}

fn unescape(s: &str) -> Result<String, String> {
    percent_encoding::percent_decode_str(s)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| "Invalid UTF-8 escape".into())
}

pub fn is_proxy_type(typ: &str) -> bool {
    matches!(
        typ,
        "socks"
            | "http"
            | "tuic"
            | "hysteria2"
            | "vless"
            | "vmess"
            | "trojan"
            | "shadowsocks"
            | "anytls"
            | "ssh"
    )
}

pub fn parse_subscription(text: &str) -> Result<ParseReport, String> {
    let text = text.trim().trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return Err("Subscription content is empty".into());
    }
    // Decode the envelope once, then dispatch by structure. URI credentials may contain Base64 too.
    let content = if !text.contains("://") && !text.starts_with('{') && !text.starts_with('[') {
        decoded(text).unwrap_or_else(|_| text.to_owned())
    } else {
        text.to_owned()
    };
    let content = content.trim().trim_start_matches('\u{feff}').trim();
    let mut report = ParseReport::default();
    if let Ok(obj) = serde_json::from_str::<Value>(content) {
        let nodes = obj
            .get("outbounds")
            .and_then(Value::as_array)
            .ok_or("JSON subscription must contain an outbounds array")?;
        for (i, node) in nodes.iter().enumerate() {
            let typ = node["type"].as_str().unwrap_or("");
            if matches!(typ, "selector" | "urltest" | "direct" | "block" | "dns") {
                continue;
            }
            let mut node = node.clone();
            if typ == "ss" {
                node["type"] = json!("shadowsocks");
            }
            let result = validate_outbound(&node).map(|_| node);
            report.add(i + 1, typ, result);
        }
    } else if content.lines().any(|l| {
        l.trim().split_once("://").is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
        })
    }) {
        for (i, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let scheme = line.split_once("://").map(|(s, _)| s).unwrap_or("unknown");
            // Do not echo arbitrary text from the subscription into diagnostics.
            let protocol = if matches!(
                scheme,
                "ss" | "socks"
                    | "socks5"
                    | "http"
                    | "https"
                    | "tuic"
                    | "hysteria2"
                    | "hy2"
                    | "vless"
            ) {
                scheme
            } else {
                "unknown"
            };
            let result = parse_uri(line, &mut report.diagnostics, i + 1);
            report.add(i + 1, protocol, result);
        }
    } else {
        let yaml: serde_yaml::Value = serde_yaml::from_str(content).map_err(|_| {
            "Unrecognized subscription format (expected JSON, Clash YAML or URI list)".to_string()
        })?;
        let nodes = yaml
            .get("proxies")
            .and_then(serde_yaml::Value::as_sequence)
            .ok_or("Unrecognized subscription format (expected JSON, Clash YAML or URI list)")?;
        for (i, node) in nodes.iter().enumerate() {
            let result = clash_outbound(node);
            report.add(i + 1, "clash", result);
        }
    }
    // Keep names unique, including collisions with generated suffixes and built-in groups.
    let mut used: HashSet<String> = ["proxy", "_dns", "direct"]
        .into_iter()
        .map(String::from)
        .collect();
    for node in &mut report.outbounds {
        let name = node["tag"].as_str().unwrap().to_owned();
        let mut tag = name.clone();
        let mut n = 2;
        while !used.insert(tag.clone()) {
            tag = format!("{} ({})", name, n);
            n += 1;
        }
        node["tag"] = json!(tag);
    }
    if report.outbounds.is_empty() {
        let details = report
            .diagnostics
            .iter()
            .take(20)
            .map(|d| format!("line {} [{}]: {}", d.line, d.protocol, d.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "Subscription content contains no supported nodes{}{}",
            if details.is_empty() { "" } else { ": " },
            details
        ));
    }
    Ok(report)
}

impl ParseReport {
    fn add(&mut self, line: usize, protocol: &str, result: Result<Value, String>) {
        match result {
            Ok(node) => self.outbounds.push(node),
            Err(message) => self.diagnostics.push(Diagnostic {
                line,
                protocol: if is_proxy_type(protocol)
                    || matches!(protocol, "ss" | "socks5" | "https" | "hy2" | "clash")
                {
                    protocol.into()
                } else {
                    "unknown".into()
                },
                message,
            }),
        }
    }
}

pub fn validate_outbound(node: &Value) -> Result<(), String> {
    let typ = node["type"].as_str().ok_or("Missing outbound type")?;
    if !is_proxy_type(typ) {
        return Err("Unsupported outbound type".into());
    }
    for key in ["tag", "server"] {
        if node[key].as_str().is_none_or(str::is_empty) {
            return Err(format!("Missing {}", key));
        }
    }
    if node["server_port"]
        .as_u64()
        .is_none_or(|p| p == 0 || p > 65535)
    {
        return Err("Invalid or missing server_port".into());
    }
    for key in match typ {
        "shadowsocks" => &["method", "password"][..],
        "tuic" => &["uuid", "password"][..],
        "vless" | "vmess" => &["uuid"][..],
        "hysteria2" | "trojan" | "anytls" => &["password"][..],
        _ => &[][..],
    } {
        if node[*key].as_str().is_none_or(str::is_empty) {
            return Err(format!("Missing {}", key));
        }
    }
    if matches!(typ, "tuic" | "vless" | "vmess") {
        uuid::Uuid::parse_str(node["uuid"].as_str().unwrap()).map_err(|_| "Invalid UUID")?;
    }
    Ok(())
}

fn flag(value: &str) -> Result<bool, String> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        _ => Err("Invalid boolean parameter".into()),
    }
}

/// Recognize proxy links without treating an ordinary subscription download URL as a proxy.
pub fn is_encoded_http_proxy_uri(text: &str) -> bool {
    if text.lines().count() != 1 {
        return false;
    }
    let Some((scheme, payload)) = text.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    decoded(payload).is_ok_and(|s| {
        Url::parse(&format!("{}://{}", scheme, s)).is_ok_and(|u| {
            !u.username().is_empty()
                && u.host_str().is_some()
                && u.port_or_known_default().is_some()
        })
    })
}

fn parse_uri(
    line: &str,
    diagnostics: &mut Vec<Diagnostic>,
    line_number: usize,
) -> Result<Value, String> {
    let (scheme, payload) = line.split_once("://").ok_or("Missing URI scheme")?;
    if !matches!(
        scheme,
        "ss" | "socks" | "socks5" | "http" | "https" | "tuic" | "hysteria2" | "hy2" | "vless"
    ) {
        return Err("Unsupported URI scheme".into());
    }
    // Some providers encode the entire HTTPS authority, path and fragment as one Base64 payload.
    let normalized = if matches!(scheme, "http" | "https") && !payload.contains('@') {
        match decoded(payload) {
            Ok(s) if s.contains('@') => format!("{}://{}", scheme, s),
            _ => line.to_owned(),
        }
    } else if scheme == "ss" && !payload.split('#').next().unwrap_or("").contains('@') {
        let (body, fragment) = payload.split_once('#').unwrap_or((payload, ""));
        format!("ss://{}#{}", decoded(body)?, fragment)
    } else {
        line.to_owned()
    };
    let u = Url::parse(&normalized).map_err(|_| "Invalid proxy URI")?;
    let server = u
        .host_str()
        .ok_or("Missing server")?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let port = u
        .port_or_known_default()
        .filter(|p| *p > 0)
        .ok_or("Invalid or missing port")?;
    let name = unescape(u.fragment().unwrap_or(""))?;
    let tag = if name.is_empty() {
        format!("{}:{}:{}", scheme, server, port)
    } else {
        name
    };
    let typ = match scheme {
        "ss" => "shadowsocks",
        "socks5" | "socks" => "socks",
        "https" => "http",
        "hy2" => "hysteria2",
        other => other,
    };
    let mut node = json!({"type":typ,"tag":tag,"server":server,"server_port":port});
    let username = unescape(u.username())?;
    let password = u.password().map(unescape).transpose()?;
    let mut q = HashMap::new();
    // Query values use URI percent encoding; a literal '+' in a password/key is not a space.
    for pair in u.query().unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let k = unescape(k)?;
        if q.insert(k, unescape(v)?).is_some() {
            return Err("Duplicate URI parameter".into());
        }
    }
    let allowed: &[&str] = match scheme {
        "ss" => &["plugin"],
        "socks" | "socks5" | "http" | "https" => {
            &["sni", "peer", "insecure", "allow_insecure", "alpn"]
        }
        "tuic" => &[
            "congestion_control",
            "alpn",
            "sni",
            "allow_insecure",
            "insecure",
            "udp_relay_mode",
            "zero_rtt_handshake",
            "heartbeat",
        ],
        "hysteria2" | "hy2" => &[
            "sni",
            "peer",
            "insecure",
            "allow_insecure",
            "alpn",
            "obfs",
            "obfs-password",
            "fastopen",
            "upmbps",
            "downmbps",
        ],
        "vless" => &[
            "security",
            "type",
            "fp",
            "pbk",
            "sid",
            "sni",
            "alpn",
            "encryption",
            "flow",
            "host",
            "path",
            "serviceName",
            "insecure",
            "allow_insecure",
        ],
        _ => &[],
    };
    if q.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("Unsupported URI parameter".into());
    }
    if u.path() != "" && u.path() != "/" {
        return Err("Unsupported proxy URI path".into());
    }
    match typ {
        "socks" | "http" => {
            if typ == "socks" {
                node["version"] = json!("5");
            }
            if !username.is_empty() {
                node["username"] = json!(username);
            }
            if let Some(p) = password {
                node["password"] = json!(p);
            }
            if scheme == "https" {
                node["tls"] = tls(&q)?;
            } else if !q.is_empty() {
                return Err("TLS parameters require HTTPS".into());
            }
        }
        "shadowsocks" => {
            let (method, pass) = if let Some(pass) = password {
                (username, pass)
            } else {
                let s = decoded(&username)?;
                let (m, p) = s.split_once(':').ok_or("Invalid SS credentials")?;
                (m.to_owned(), p.to_owned())
            };
            node["method"] = json!(method);
            node["password"] = json!(pass);
            if let Some(plugin) = q.get("plugin") {
                let (name, opts) = plugin.split_once(';').unwrap_or((plugin, ""));
                // Never silently remove a plugin: that produces a node with different wire semantics.
                if !matches!(name, "obfs-local" | "simple-obfs" | "v2ray-plugin") {
                    return Err("Unsupported SS plugin".into());
                }
                node["plugin"] = json!(if name == "simple-obfs" {
                    "obfs-local"
                } else {
                    name
                });
                if !opts.is_empty() {
                    node["plugin_opts"] = json!(opts);
                }
            }
        }
        "tuic" => {
            node["uuid"] = json!(username);
            node["password"] = json!(password.ok_or("Missing TUIC password")?);
            node["tls"] = tls(&q)?;
            for key in ["congestion_control", "udp_relay_mode", "heartbeat"] {
                if let Some(v) = q.get(key) {
                    node[key] = json!(v);
                }
            }
            if let Some(v) = q.get("congestion_control") {
                if !matches!(v.as_str(), "bbr" | "cubic" | "new_reno") {
                    return Err("Invalid TUIC congestion_control".into());
                }
            }
            if let Some(v) = q.get("udp_relay_mode") {
                if !matches!(v.as_str(), "native" | "quic") {
                    return Err("Invalid TUIC udp_relay_mode".into());
                }
            }
            if let Some(v) = q.get("zero_rtt_handshake") {
                node["zero_rtt_handshake"] = json!(flag(v)?);
            }
        }
        "hysteria2" => {
            node["password"] = json!(if let Some(pass) = password {
                format!("{}:{}", username, pass)
            } else {
                username
            });
            node["tls"] = tls(&q)?;
            if let Some(obfs) = q.get("obfs") {
                if !matches!(obfs.as_str(), "salamander" | "gecko") {
                    return Err("Unsupported Hysteria2 obfuscation".into());
                }
                node["obfs"] = json!({"type":obfs,"password":q.get("obfs-password").ok_or("Missing obfs-password")?});
            } else if q.contains_key("obfs-password") {
                return Err("obfs-password requires obfs".into());
            }
            for (key, target) in [("upmbps", "up_mbps"), ("downmbps", "down_mbps")] {
                if let Some(v) = q.get(key) {
                    let n: u32 = v.parse().map_err(|_| "Invalid bandwidth")?;
                    if n == 0 {
                        return Err("Bandwidth must be positive".into());
                    }
                    node[target] = json!(n);
                }
            }
            if let Some(v) = q.get("fastopen") {
                flag(v)?;
                diagnostics.push(Diagnostic {
                    line: line_number,
                    protocol: scheme.into(),
                    message: "fastopen ignored: Hysteria2 uses QUIC, not TCP".into(),
                });
            }
        }
        "vless" => {
            if password.is_some() {
                return Err("VLESS credentials must contain only a UUID".into());
            }
            node["uuid"] = json!(username);
            if q.get("encryption").is_some_and(|v| v != "none") {
                return Err("Unsupported VLESS encryption".into());
            }
            if let Some(flow) = q.get("flow") {
                if !flow.is_empty() && flow != "xtls-rprx-vision" {
                    return Err("Unsupported VLESS flow".into());
                }
                node["flow"] = json!(flow);
            }
            let security = q.get("security").map(String::as_str).unwrap_or("none");
            match security {
                "tls" | "reality" => {
                    let mut t = tls(&q)?;
                    if let Some(fp) = q.get("fp") {
                        t["utls"] = json!({"enabled":true,"fingerprint":fp});
                    }
                    if security == "reality" {
                        let public_key = q.get("pbk").ok_or("Missing Reality public key")?;
                        if decode_base64(public_key)?.len() != 32 {
                            return Err("Invalid Reality public key".into());
                        }
                        let sid = q.get("sid").map(String::as_str).unwrap_or("");
                        if sid.len() > 16
                            || sid.len() % 2 != 0
                            || !sid.bytes().all(|b| b.is_ascii_hexdigit())
                        {
                            return Err("Invalid Reality short ID".into());
                        }
                        t["reality"] =
                            json!({"enabled":true,"public_key":public_key,"short_id":sid});
                        if !q.contains_key("fp") {
                            t["utls"] = json!({"enabled":true,"fingerprint":"chrome"});
                        }
                    } else if q.contains_key("pbk") || q.contains_key("sid") {
                        return Err("Reality parameters require security=reality".into());
                    }
                    node["tls"] = t;
                }
                "none" | "" => {
                    if [
                        "sni",
                        "fp",
                        "pbk",
                        "sid",
                        "alpn",
                        "insecure",
                        "allow_insecure",
                    ]
                    .iter()
                    .any(|k| q.contains_key(*k))
                    {
                        return Err("TLS parameters require TLS or Reality".into());
                    }
                }
                _ => return Err("Unsupported VLESS security".into()),
            }
            match q.get("type").map(String::as_str).unwrap_or("tcp") {
                "tcp" => {
                    if q.contains_key("host")
                        || q.contains_key("path")
                        || q.contains_key("serviceName")
                    {
                        return Err("Transport parameters are not applicable to TCP".into());
                    }
                }
                "ws" => {
                    if q.contains_key("serviceName") {
                        return Err("serviceName requires gRPC".into());
                    }
                    let mut transport = json!({"type":"ws","path":q.get("path").map(String::as_str).unwrap_or("/")});
                    if let Some(host) = q.get("host") {
                        transport["headers"] = json!({"Host":host});
                    }
                    node["transport"] = transport;
                }
                "grpc" => {
                    if q.contains_key("host") || q.contains_key("path") {
                        return Err("HTTP parameters are not applicable to gRPC".into());
                    }
                    node["transport"] = json!({"type":"grpc","service_name":q.get("serviceName").map(String::as_str).unwrap_or("")});
                }
                _ => return Err("Unsupported VLESS transport".into()),
            }
        }
        _ => unreachable!(),
    }
    validate_outbound(&node)?;
    Ok(node)
}

fn tls(q: &HashMap<String, String>) -> Result<Value, String> {
    let mut t = json!({"enabled":true});
    if let (Some(sni), Some(peer)) = (q.get("sni"), q.get("peer")) {
        if sni != peer {
            return Err("Conflicting TLS server names".into());
        }
    }
    if let Some(sni) = q.get("sni").or_else(|| q.get("peer")) {
        t["server_name"] = json!(sni);
    }
    let insecure = q.get("insecure").map(|v| flag(v)).transpose()?;
    let allow = q.get("allow_insecure").map(|v| flag(v)).transpose()?;
    if insecure.is_some() && allow.is_some() && insecure != allow {
        return Err("Conflicting TLS verification options".into());
    }
    if let Some(v) = insecure.or(allow) {
        t["insecure"] = json!(v);
    }
    if let Some(alpn) = q.get("alpn") {
        t["alpn"] = json!(alpn.split(',').collect::<Vec<_>>());
    }
    Ok(t)
}

fn clash_outbound(yaml: &serde_yaml::Value) -> Result<Value, String> {
    let c = serde_json::to_value(yaml).map_err(|_| "Invalid Clash node")?;
    let typ = match c["type"].as_str().unwrap_or("") {
        "ss" => "shadowsocks",
        "hysteria2" => "hysteria2",
        "anytls" => "anytls",
        _ => return Err("Unsupported Clash node type".into()),
    };
    let mut node = json!({"type":typ,"tag":c["name"],"server":c["server"],"server_port":c["port"],"password":c["password"]});
    if typ == "shadowsocks" {
        node["method"] = c["cipher"].clone();
        if !c["plugin"].is_null() {
            return Err(
                "Clash SS plugins require explicit conversion; use an SS URI or sing-box JSON"
                    .into(),
            );
        }
    } else {
        let mut t = json!({"enabled":true});
        for (source, target) in [
            ("sni", "server_name"),
            ("skip-cert-verify", "insecure"),
            ("alpn", "alpn"),
        ] {
            if !c[source].is_null() {
                t[target] = c[source].clone();
            }
        }
        node["tls"] = t;
        if typ == "hysteria2" {
            if let Some(obfs) = c["obfs"].as_str() {
                node["obfs"] = json!({"type":obfs,"password":c["obfs-password"]});
            }
            // Clash up/down may include units. Do not silently replace them with fixed bandwidths.
            for (source, target) in [("up", "up_mbps"), ("down", "down_mbps")] {
                if !c[source].is_null() {
                    let v = c[source]
                        .as_u64()
                        .or_else(|| c[source].as_str().and_then(|v| v.parse().ok()))
                        .ok_or("Unsupported Clash bandwidth unit; use Mbps number")?;
                    node[target] = json!(v);
                }
            }
        }
    }
    validate_outbound(&node)?;
    Ok(node)
}
