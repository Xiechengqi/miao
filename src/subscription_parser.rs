use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowsocksOutbound {
    #[serde(rename = "type")]
    pub outbound_type: String,
    pub tag: String,
    pub server: String,
    pub server_port: u16,
    pub method: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_opts: Option<String>,
}

pub fn parse_subscription(base64_content: &str) -> Result<Vec<ShadowsocksOutbound>, String> {
    let decoded = general_purpose::STANDARD
        .decode(base64_content.trim())
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    let content = String::from_utf8(decoded)
        .map_err(|e| format!("UTF-8 decode error: {}", e))?;

    let mut outbounds = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with("ss://") {
            continue;
        }

        if let Ok(outbound) = parse_ss_url(line) {
            outbounds.push(outbound);
        }
    }

    Ok(outbounds)
}

fn parse_ss_url(url_str: &str) -> Result<ShadowsocksOutbound, String> {
    let url = Url::parse(url_str)
        .map_err(|e| format!("URL parse error: {}", e))?;

    let userinfo = url.username();
    let decoded_userinfo = general_purpose::STANDARD
        .decode(userinfo)
        .map_err(|e| format!("Userinfo decode error: {}", e))?;
    let userinfo_str = String::from_utf8(decoded_userinfo)
        .map_err(|e| format!("Userinfo UTF-8 error: {}", e))?;

    let parts: Vec<&str> = userinfo_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err("Invalid userinfo format".to_string());
    }

    let method = parts[0].to_string();
    let password = parts[1].to_string();

    let server = url.host_str()
        .ok_or("Missing host")?
        .to_string();
    let server_port = url.port()
        .ok_or("Missing port")?;

    let tag = urlencoding::decode(url.fragment().unwrap_or(""))
        .map_err(|e| format!("Fragment decode error: {}", e))?
        .to_string();

    let query_pairs: HashMap<String, String> = url.query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let (plugin, plugin_opts) = if let Some(plugin_str) = query_pairs.get("plugin") {
        parse_plugin(plugin_str)?
    } else {
        (None, None)
    };

    Ok(ShadowsocksOutbound {
        outbound_type: "shadowsocks".to_string(),
        tag,
        server,
        server_port,
        method,
        password,
        plugin,
        plugin_opts,
    })
}

fn parse_plugin(plugin_str: &str) -> Result<(Option<String>, Option<String>), String> {
    let parts: Vec<&str> = plugin_str.split(';').collect();
    if parts.is_empty() {
        return Ok((None, None));
    }

    let plugin_name = match parts[0] {
        "simple-obfs" => "obfs-local",
        other => other,
    };

    let opts = if parts.len() > 1 {
        Some(parts[1..].join(";"))
    } else {
        None
    };

    Ok((Some(plugin_name.to_string()), opts))
}
