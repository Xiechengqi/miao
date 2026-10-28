//! Inspect a local subscription without printing credentials.
//! cargo run --example parse_subscription -- /path/to/subscription /tmp/outbounds.json
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let report = miao_rust::subscription_parser::parse_subscription(&fs::read_to_string(
        args.get(1).ok_or("Missing input file")?,
    )?)?;
    let mut counts = std::collections::BTreeMap::new();
    for node in &report.outbounds {
        *counts.entry(node["type"].as_str().unwrap()).or_insert(0) += 1;
    }
    println!(
        "Parsed {} nodes: {:?}; {} diagnostics",
        report.outbounds.len(),
        counts,
        report.diagnostics.len()
    );
    if let Some(path) = args.get(2) {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(
            serde_json::to_string(&serde_json::json!({"outbounds":report.outbounds}))?.as_bytes(),
        )?;
    }
    Ok(())
}
