//! Latency probes: real proxy (via chaos-prober) with TCP connect fallback.

use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
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
///
/// DNS resolve and TCP connect share one wall-clock budget so a stuck
/// resolver cannot exceed `connect_timeout` per node.
pub async fn probe_tcp(addr: &str, connect_timeout: Duration) -> Result<u32, String> {
    let addr = addr.trim();
    if addr.is_empty() {
        return Err("empty address".to_string());
    }

    let start = Instant::now();
    match timeout(connect_timeout, async {
        let socket_addr = resolve_addr(addr).await?;
        TcpStream::connect(socket_addr)
            .await
            .map_err(|e| format!("connect failed: {e}"))
    })
    .await
    {
        Ok(Ok(_stream)) => {
            let ms = start.elapsed().as_millis();
            let ms = u32::try_from(ms).unwrap_or(u32::MAX);
            Ok(ms)
        }
        Ok(Err(e)) => Err(e),
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
    matches!(
        s.rsplit_once(':'),
        Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok()
    )
}

const DEFAULT_CONCURRENCY: usize = 20;
const DEFAULT_CHECK_URL: &str = "http://cp.cloudflare.com";

/// JSON request sent to chaos-prober stdin.
#[derive(Debug, Serialize)]
struct ProberRequest {
    links: Vec<String>,
    url: String,
    timeout_ms: u64,
    concurrency: usize,
}

/// JSON response read from chaos-prober stdout.
#[derive(Debug, Deserialize)]
struct ProberResponse {
    results: Vec<ProberResult>,
}

/// Single result from chaos-prober.
#[derive(Debug, Deserialize)]
struct ProberResult {
    link: String,
    latency_ms: Option<i64>,
    alive: bool,
    #[serde(default)]
    error: Option<String>,
}

/// Probe method reported in LatencySample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMethod {
    /// Real proxy latency via chaos-prober (HTTP HEAD through proxy).
    Proxy,
    /// TCP connect only (fallback when prober unavailable).
    Tcp,
}

impl ProbeMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeMethod::Proxy => "proxy",
            ProbeMethod::Tcp => "tcp",
        }
    }
}

/// Upper bound on how long the parent waits for the prober, independent of how
/// many links were requested.
const PROBER_MAX_BUDGET: Duration = Duration::from_secs(600);

/// Slack added to the per-connect budget for process start-up and I/O.
const PROBER_STARTUP_SLACK: Duration = Duration::from_secs(10);

/// How long to wait for `chaos-prober` before killing it.
///
/// The prober receives a per-connect timeout, but it probes in batches, so the
/// wall-clock budget scales with the number of batches rather than with a single
/// connect timeout. Bounding it here is what stops a wedged prober from holding
/// a request open indefinitely.
fn prober_budget(targets: usize, connect_timeout: Duration) -> Duration {
    let batches = targets.div_ceil(DEFAULT_CONCURRENCY).max(1) as u32;
    connect_timeout
        .saturating_mul(batches)
        .saturating_add(PROBER_STARTUP_SLACK)
        .min(PROBER_MAX_BUDGET)
}

/// Probe nodes via chaos-prober subprocess for real proxy latency.
///
/// `targets`: vec of `(node_id, link)`.
/// Returns samples with `ProbeMethod::Proxy`.
pub async fn probe_via_prober(
    prober_bin: &Path,
    targets: Vec<(String, String)>,
    connect_timeout: Duration,
    tested_at: &str,
) -> Result<Vec<LatencySample>, String> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let links: Vec<String> = targets.iter().map(|(_, link)| link.clone()).collect();
    let req = ProberRequest {
        links: links.clone(),
        url: DEFAULT_CHECK_URL.to_string(),
        timeout_ms: connect_timeout.as_millis() as u64,
        concurrency: DEFAULT_CONCURRENCY,
    };

    let input = serde_json::to_string(&req).map_err(|e| format!("serialize request: {e}"))?;

    let output = tokio::process::Command::new(prober_bin)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn prober: {e}"))?;

    // Write to stdin and collect stdout.
    use tokio::io::AsyncWriteExt;
    let mut child = output;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| format!("write stdin: {e}"))?;
        drop(stdin); // Close stdin so prober sees EOF.
    }

    let out = match timeout(
        prober_budget(targets.len(), connect_timeout),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("wait prober: {e}")),
        Err(_) => {
            // `kill_on_drop` only fires when the future owning the child is
            // dropped, which never happens while we simply await it — so a hung
            // prober used to pin this task (and its child) forever. The timeout
            // drops the `wait_with_output` future and with it the child, which
            // SIGKILLs the process.
            return Err(format!(
                "prober did not finish within {}s",
                prober_budget(targets.len(), connect_timeout).as_secs()
            ));
        }
    };

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("prober exited {}: {}", out.status, stderr.trim()));
    }

    let resp: ProberResponse =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("parse prober output: {e}"))?;

    // A subscription may contain the same link more than once. Preserve every
    // node id in request order rather than collapsing duplicates into one map entry.
    let mut link_to_nodes: std::collections::HashMap<String, std::collections::VecDeque<String>> =
        std::collections::HashMap::new();
    for (node_id, link) in &targets {
        link_to_nodes
            .entry(link.clone())
            .or_default()
            .push_back(node_id.clone());
    }

    let mut samples = Vec::with_capacity(resp.results.len());
    for r in resp.results {
        let node_id = link_to_nodes
            .get_mut(&r.link)
            .and_then(std::collections::VecDeque::pop_front)
            .ok_or_else(|| format!("prober returned an unexpected result for link {}", r.link))?;
        samples.push(LatencySample {
            node_id,
            latency_ms: r.latency_ms.and_then(|v| u32::try_from(v).ok()),
            alive: r.alive,
            tested_at: tested_at.to_string(),
            message: r.error,
        });
    }

    let missing = link_to_nodes
        .values()
        .map(std::collections::VecDeque::len)
        .sum::<usize>();
    if missing > 0 {
        return Err(format!("prober omitted {missing} requested result(s)"));
    }

    Ok(samples)
}

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

    #[test]
    fn prober_budget_scales_with_batches_and_is_capped() {
        let per_connect = Duration::from_secs(5);

        // A single batch costs one connect timeout plus start-up slack.
        assert_eq!(
            prober_budget(1, per_connect),
            Duration::from_secs(5) + PROBER_STARTUP_SLACK
        );
        // Concurrency is 20, so 21 links need two batches.
        assert_eq!(
            prober_budget(21, per_connect),
            Duration::from_secs(10) + PROBER_STARTUP_SLACK
        );
        // No target at all still needs one batch's worth of budget.
        assert_eq!(
            prober_budget(0, per_connect),
            Duration::from_secs(5) + PROBER_STARTUP_SLACK
        );
        // A huge node list must not produce an unbounded wait.
        assert_eq!(prober_budget(100_000, per_connect), PROBER_MAX_BUDGET);
    }

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
        // NXDOMAIN / unresolvable host — must fail (TEST-NET can be hijacked by local proxies).
        let err = probe_tcp(
            "this-host-should-not-exist.invalid:1",
            Duration::from_millis(400),
        )
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
