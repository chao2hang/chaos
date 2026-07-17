//! TCP connect latency probe (MVP — not full proxy path).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Domain latency sample for a single node probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatencySample {
    pub node_id: String,
    pub latency_ms: Option<u32>,
    pub alive: bool,
    pub tested_at: String,
    pub message: Option<String>,
}

/// TCP connect to `host:port` (or bare host — invalid without port).
/// Returns elapsed milliseconds on success, or an error message.
pub async fn probe_tcp(addr: &str, connect_timeout: Duration) -> Result<u32, String> {
    let addr = addr.trim();
    if addr.is_empty() {
        return Err("empty address".to_string());
    }

    let socket_addr = resolve_addr(addr).await?;

    let start = Instant::now();
    match timeout(connect_timeout, TcpStream::connect(socket_addr)).await {
        Ok(Ok(_stream)) => {
            let ms = start.elapsed().as_millis();
            let ms = u32::try_from(ms).unwrap_or(u32::MAX);
            Ok(ms)
        }
        Ok(Err(e)) => Err(format!("connect failed: {e}")),
        Err(_) => Err(format!("timeout after {}ms", connect_timeout.as_millis())),
    }
}

async fn resolve_addr(addr: &str) -> Result<SocketAddr, String> {
    // Prefer direct parse for host:port / IPv4 / IPv6 with port.
    if let Ok(sa) = addr.parse::<SocketAddr>() {
        return Ok(sa);
    }

    // tokio::net::lookup_host needs host:port form.
    let mut iter = tokio::net::lookup_host(addr)
        .await
        .map_err(|e| format!("resolve failed: {e}"))?;
    iter.next()
        .ok_or_else(|| format!("no addresses resolved for {addr}"))
}

/// Best-effort host:port from stored `address` or share `link`.
pub fn resolve_probe_target(address: Option<&str>, link: &str) -> Option<String> {
    if let Some(a) = address.map(str::trim).filter(|s| !s.is_empty()) {
        if looks_like_hostport(a) {
            return Some(a.to_string());
        }
    }
    crate::link::detect_address(link).filter(|a| looks_like_hostport(a))
}

fn looks_like_hostport(s: &str) -> bool {
    // Require a port so TcpStream::connect / lookup_host has something usable.
    if s.parse::<SocketAddr>().is_ok() {
        return true;
    }
    // host:port (last colon; skip bare IPv6 without brackets)
    match s.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok() => true,
        _ => false,
    }
}

const DEFAULT_CONCURRENCY: usize = 20;

/// Probe many targets concurrently (max 20 in flight). `timeout` per connect.
pub async fn probe_batch(
    targets: Vec<(String, String)>,
    connect_timeout: Duration,
    tested_at: &str,
) -> Vec<LatencySample> {
    stream::iter(targets)
        .map(|(node_id, addr)| {
            let tested_at = tested_at.to_string();
            async move {
                match probe_tcp(&addr, connect_timeout).await {
                    Ok(ms) => LatencySample {
                        node_id,
                        latency_ms: Some(ms),
                        alive: true,
                        tested_at,
                        message: None,
                    },
                    Err(msg) => LatencySample {
                        node_id,
                        latency_ms: None,
                        alive: false,
                        tested_at,
                        message: Some(msg),
                    },
                }
            }
        })
        .buffer_unordered(DEFAULT_CONCURRENCY)
        .collect()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn probe_tcp_localhost() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
        });
        let ms = crate::latency::probe_tcp(
            &format!("127.0.0.1:{port}"),
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert!(ms < 2000);
    }

    #[tokio::test]
    async fn probe_tcp_timeout_unreachable() {
        // RFC 5737 TEST-NET-1: typically unroutable / filtered; use short timeout.
        let err = probe_tcp("192.0.2.1:1", Duration::from_millis(200))
            .await
            .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn resolve_probe_target_prefers_address() {
        assert_eq!(
            resolve_probe_target(Some("1.2.3.4:443"), "trojan://x@9.9.9.9:80").as_deref(),
            Some("1.2.3.4:443")
        );
        assert_eq!(
            resolve_probe_target(None, "trojan://x@9.9.9.9:80").as_deref(),
            Some("9.9.9.9:80")
        );
        assert_eq!(resolve_probe_target(Some("onlyhost"), "ss://payload"), None);
    }
}
