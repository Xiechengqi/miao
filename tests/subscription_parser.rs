use base64::{engine::general_purpose as b64, Engine as _};
use miao_rust::subscription_parser::{
    decode_base64, is_encoded_http_proxy_uri, parse_subscription,
};
use serde_json::{json, Value};

const ID: &str = "00000000-0000-4000-8000-000000000001";
fn one(uri: &str) -> Value {
    parse_subscription(uri).unwrap().outbounds.remove(0)
}

#[test]
fn provider_envelope_parses_all_37_nodes() {
    let report = parse_subscription(include_str!("fixtures/mixed-subscription.txt")).unwrap();
    let count = |typ| report.outbounds.iter().filter(|n| n["type"] == typ).count();
    assert_eq!(report.outbounds.len(), 37);
    assert_eq!(
        (
            count("socks"),
            count("http"),
            count("tuic"),
            count("hysteria2"),
            count("vless")
        ),
        (4, 24, 4, 4, 1)
    );
    assert_eq!(report.diagnostics.len(), 4); // QUIC ignores the provider's TCP fastopen flag.
    assert_eq!(report.outbounds[4]["password"], "p:a+ss");
    assert_eq!(report.outbounds[4]["tls"]["enabled"], true);
    let vless = report.outbounds.last().unwrap();
    assert_eq!(vless["tls"]["reality"]["enabled"], true);
    assert!(vless.get("transport").is_none());
}

#[test]
fn base64_variants_and_whitespace() {
    let input = "socks5://user:pass@proxy.example:443#你好";
    for engine in [
        b64::STANDARD,
        b64::STANDARD_NO_PAD,
        b64::URL_SAFE,
        b64::URL_SAFE_NO_PAD,
    ] {
        let encoded = engine.encode(input);
        let wrapped = encoded
            .as_bytes()
            .chunks(13)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n ");
        assert_eq!(one(&wrapped)["tag"], "你好");
    }
    assert!(decode_base64("not!base64").is_err());
}

#[test]
fn literal_plus_and_percent_escapes_are_decoded_once() {
    let node = one("socks5://user:p%253A+%3A@proxy.example:443#name%2520%2B");
    assert_eq!(node["password"], "p%3A+:");
    assert_eq!(node["tag"], "name%20+");
    let node = one("hy2://p%2Bass@proxy.example:443?sni=proxy.example&insecure=0");
    assert_eq!(node["password"], "p+ass");
    assert_eq!(node["tls"]["insecure"], false);
}

#[test]
fn userpass_hysteria_and_obfs_are_preserved() {
    let node = one("hysteria2://user:pa%3Ass@proxy.example:443?obfs=salamander&obfs-password=p%2Bass&upmbps=100&downmbps=200");
    assert_eq!(node["password"], "user:pa:ss");
    assert_eq!(node["obfs"]["password"], "p+ass");
    assert_eq!(node["up_mbps"], 100);
    assert!(node["tls"].get("insecure").is_none());
}

#[test]
fn ss_sip002_legacy_and_plain_credentials() {
    let auth = b64::STANDARD.encode("aes-128-gcm:p:a+ss");
    let node = one(&format!("ss://{}@proxy.example:443#SS", auth));
    assert_eq!(node["password"], "p:a+ss");
    let legacy = b64::STANDARD.encode("aes-128-gcm:pass@proxy.example:443");
    assert_eq!(one(&format!("ss://{}#SS", legacy))["method"], "aes-128-gcm");
    assert_eq!(
        one("ss://aes-128-gcm:p%3Aa@proxy.example:443")["password"],
        "p:a"
    );
}

#[test]
fn ss_plugins_are_preserved_instead_of_stripped() {
    let auth = b64::STANDARD_NO_PAD.encode("aes-128-gcm:pass");
    let node = one(&format!(
        "ss://{}@proxy.example:443?plugin=simple-obfs%3Bobfs%3Dhttp#SS",
        auth
    ));
    assert_eq!(node["plugin"], "obfs-local");
    assert_eq!(node["plugin_opts"], "obfs=http");
}

#[test]
fn ipv6_and_default_https_port() {
    let node = one("socks5://user:pass@[2001:db8::1]:1080#IPV6");
    assert_eq!(node["server"], "2001:db8::1");
    assert_eq!(node["server_port"], 1080);
    assert_eq!(one("https://user:pass@proxy.example")["server_port"], 443);
}

#[test]
fn vless_transport_and_tls_mapping() {
    let node = one(&format!(
        "vless://{}@proxy.example:443?security=tls&type=ws&host=cdn.example&path=%2Fws&fp=chrome",
        ID
    ));
    assert_eq!(
        node["transport"],
        json!({"type":"ws","path":"/ws","headers":{"Host":"cdn.example"}})
    );
    let node = one(&format!(
        "vless://{}@proxy.example:443?security=tls&type=grpc&serviceName=tunnel",
        ID
    ));
    assert_eq!(node["transport"]["service_name"], "tunnel");
    assert!(parse_subscription(&format!("vless://{}@proxy.example:443?type=xhttp", ID)).is_err());
}

#[test]
fn vless_is_never_misclassified_as_ss() {
    let node = one(&format!("vless://{}@proxy.example:443?encryption=none", ID));
    assert_eq!(node["type"], "vless");
}

#[test]
fn names_are_unique_and_do_not_shadow_groups() {
    let nodes=parse_subscription("socks5://p.example:443#proxy\nsocks5://p.example:443#proxy\nsocks5://p.example:443#proxy%20(2)").unwrap().outbounds;
    let tags = nodes
        .iter()
        .map(|n| n["tag"].as_str().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(tags.len(), 3);
    assert!(!tags.contains("proxy"));
}

#[test]
fn partial_failures_have_safe_line_diagnostics() {
    let report=parse_subscription("socks5://p.example:443#ok\nsocks5://private:TOPSECRET@p.example:0#SECRET-NAME\nfoo://private:TOPSECRET@p.example:1").unwrap();
    assert_eq!(report.outbounds.len(), 1);
    assert_eq!(report.diagnostics.len(), 2);
    assert_eq!(report.diagnostics[0].line, 2);
    let diagnostic = serde_json::to_string(&report.diagnostics).unwrap();
    assert!(!diagnostic.contains("TOPSECRET"));
    assert!(!diagnostic.contains("SECRET-NAME"));
    let error = parse_subscription("socks5://private:TOPSECRET@p.example:0").unwrap_err();
    assert!(error.contains("line 1"));
    assert!(!error.contains("TOPSECRET"));
}

#[test]
fn malformed_and_unsupported_options_are_rejected() {
    for uri in [
        "socks5://p.example:0",
        "socks5://p.example:65536",
        "hysteria2://p.example:443",
        "tuic://bad:pass@p.example:443",
        "hy2://pass@p.example:443?insecure=yes",
        "hy2://pass@p.example:443?insecure=1&allow_insecure=0",
        "hy2://pass@p.example:443?insecure=1&insecure=0",
        "hy2://pass@p.example:443?unsupported=secret",
        "https://u:p@p.example/path",
        "socks5://u:p@p.example:443?insecure=1",
    ] {
        assert!(
            parse_subscription(uri).is_err(),
            "expected failure for synthetic URI"
        );
    }
}

#[test]
fn json_preserves_hysteria_semantics_and_includes_new_types() {
    let hy = json!({"type":"hysteria2","tag":"HY","server":"p.example","server_port":443,"password":"pass","tls":{"enabled":true,"insecure":false,"alpn":["h3"]},"obfs":{"type":"salamander","password":"obfs"},"up_mbps":15});
    let input = json!({"outbounds":[hy.clone(),{"type":"socks","tag":"SOCKS","server":"p.example","server_port":443},{"type":"http","tag":"HTTP","server":"p.example","server_port":443},{"type":"tuic","tag":"TUIC","server":"p.example","server_port":443,"uuid":ID,"password":"pass","tls":{"enabled":true}},{"type":"direct","tag":"direct"}]});
    let report = parse_subscription(&input.to_string()).unwrap();
    assert_eq!(report.outbounds.len(), 4);
    assert_eq!(report.outbounds[0], hy);
}

#[test]
fn clash_yaml_and_base64_json_use_shared_parser() {
    let yaml="proxies:\n  - name: HY\n    type: hysteria2\n    server: p.example\n    port: 443\n    password: secret\n    sni: p.example\n    skip-cert-verify: false\n    obfs: salamander\n    obfs-password: pass\n";
    let node = one(yaml);
    assert_eq!(node["tls"]["insecure"], false);
    assert!(node.get("up_mbps").is_none());
    let encoded = b64::STANDARD.encode(json!({"outbounds":[node.clone()]}).to_string());
    assert_eq!(one(&encoded), node);
    let yaml="proxies:\n  - name: SS\n    type: ss\n    server: p.example\n    port: 443\n    cipher: aes-128-gcm\n    password: pass\n";
    assert_eq!(one(yaml)["type"], "shadowsocks");
}

#[test]
fn proxy_links_are_distinguished_from_download_urls() {
    assert!(!is_encoded_http_proxy_uri(
        "https://provider.example/sub?token=secret"
    ));
    assert!(!is_encoded_http_proxy_uri("https://u:p@p.example:443"));
    assert!(is_encoded_http_proxy_uri(&format!(
        "https://{}",
        b64::STANDARD.encode("user:pass@p.example:443/#node")
    )));
}
