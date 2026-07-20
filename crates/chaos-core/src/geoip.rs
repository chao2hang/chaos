//! GeoIP lookup via ip-api.com batch API.
//!
//! Free tier: 45 req/min for single, 15 req/min for batch (up to 100 IPs per batch).
//! No API key required. Only use for non-commercial / self-hosted.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const BATCH_URL: &str = "http://ip-api.com/batch?fields=query,countryCode,status";
const MAX_BATCH: usize = 100;
const TIMEOUT: Duration = Duration::from_secs(10);

/// GeoIP is opt-in because node addresses are sent to a third-party service.
/// Set `CHAOS_GEOIP_ENABLED=1` (or `true`/`yes`) to enable lookups.
pub fn enabled() -> bool {
    std::env::var("CHAOS_GEOIP_ENABLED")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

#[derive(Debug, Serialize)]
struct BatchQuery {
    query: String,
}

#[derive(Debug, Deserialize)]
struct BatchResult {
    query: String,
    status: String,
    #[serde(rename = "countryCode", default)]
    country_code: Option<String>,
}

/// Extract the IP (or hostname) portion from a `host:port` address string.
fn extract_host(address: &str) -> &str {
    let addr = address.trim();
    // Handle IPv6 with brackets: [::1]:443
    if addr.starts_with('[') {
        if let Some(end) = addr.find(']') {
            return &addr[1..end];
        }
    }
    // host:port — take everything before the last colon
    // But only if what's after the colon looks like a port number
    if let Some((host, port)) = addr.rsplit_once(':') {
        if port.parse::<u16>().is_ok() && !host.is_empty() {
            return host;
        }
    }
    addr
}

/// Check if a host string is an IP address (not a domain name).
fn is_ip(host: &str) -> bool {
    host.parse::<IpAddr>().is_ok()
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(value) => {
            !value.is_private()
                && !value.is_loopback()
                && !value.is_link_local()
                && !value.is_unspecified()
                && !value.is_multicast()
                && !value.is_broadcast()
        }
        IpAddr::V6(value) => {
            !value.is_loopback()
                && !value.is_unique_local()
                && !value.is_unicast_link_local()
                && !value.is_unspecified()
                && !value.is_multicast()
        }
    }
}

/// Batch lookup country codes for a list of (node_id, address) pairs.
///
/// Returns a map of node_id → country_code (ISO 3166-1 alpha-2).
/// Only includes entries where lookup succeeded. Silently skips failures.
pub async fn batch_lookup_country(pairs: &[(String, String)]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if pairs.is_empty() {
        return result;
    }

    // Deduplicate IPs and map back to node_ids.
    let mut ip_to_nodes: HashMap<String, Vec<String>> = HashMap::new();
    let mut ips_to_query: Vec<String> = Vec::new();

    for (node_id, address) in pairs {
        let host = extract_host(address);
        if !is_ip(host) {
            // Do not disclose hostnames to the third-party lookup service.
            continue;
        }
        let Ok(ip) = host.parse::<IpAddr>() else {
            continue;
        };
        if !is_public_ip(ip) {
            continue;
        }
        let key = ip.to_string();
        ip_to_nodes
            .entry(key.clone())
            .or_default()
            .push(node_id.clone());
        if !ips_to_query.contains(&key) {
            ips_to_query.push(key);
        }
    }

    let client = match reqwest::Client::builder().timeout(TIMEOUT).build() {
        Ok(c) => c,
        Err(_) => return result,
    };

    // Process in chunks of MAX_BATCH.
    for chunk in ips_to_query.chunks(MAX_BATCH) {
        let queries: Vec<BatchQuery> = chunk
            .iter()
            .map(|ip| BatchQuery { query: ip.clone() })
            .collect();

        let resp = match client.post(BATCH_URL).json(&queries).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "GeoIP batch request failed");
                continue;
            }
        };

        let results: Vec<BatchResult> = match resp.json().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "GeoIP batch parse failed");
                continue;
            }
        };

        for r in results {
            if r.status == "success" {
                if let Some(cc) = r.country_code {
                    if let Some(node_ids) = ip_to_nodes.get(&r.query) {
                        for nid in node_ids {
                            result.insert(nid.clone(), cc.clone());
                        }
                    }
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_host_from_ip_port() {
        assert_eq!(extract_host("1.2.3.4:443"), "1.2.3.4");
        assert_eq!(extract_host("example.com:8080"), "example.com");
        assert_eq!(extract_host("[::1]:443"), "::1");
        assert_eq!(extract_host("1.2.3.4"), "1.2.3.4");
    }

    #[test]
    fn is_ip_check() {
        assert!(is_ip("1.2.3.4"));
        assert!(is_ip("::1"));
        assert!(is_ip("2606:4700:4700::1111"));
        assert!(!is_ip("example.com"));
        assert!(!is_ip("onlybase64"));
        assert!(!is_public_ip("10.0.0.1".parse().unwrap()));
        assert!(is_public_ip("1.1.1.1".parse().unwrap()));
    }
}
