//! Traffic and connection monitoring data models.
//!
//! dae exposes connection information through its log output and potentially
//! through eBPF maps. This module provides the data models for representing
//! active connections and traffic statistics.

use serde::{Deserialize, Serialize};

/// An active connection passing through dae.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveConnection {
    /// Unique connection identifier.
    pub id: String,
    /// Source address (ip:port).
    pub source: String,
    /// Destination address (ip:port or domain).
    pub destination: String,
    /// Outbound node/group name handling this connection.
    pub outbound: String,
    /// Protocol: tcp, udp, etc.
    pub protocol: String,
    /// Connection start time (RFC 3339).
    pub started_at: String,
    /// Duration in seconds.
    pub duration_secs: u64,
    /// Bytes uploaded.
    pub bytes_up: u64,
    /// Bytes downloaded.
    pub bytes_down: u64,
}

/// Traffic statistics for a time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficStats {
    /// Node ID (null for aggregate).
    pub node_id: Option<String>,
    /// Period start (RFC 3339).
    pub period_start: String,
    /// Period end (RFC 3339).
    pub period_end: String,
    /// Total bytes uploaded.
    pub bytes_up: u64,
    /// Total bytes downloaded.
    pub bytes_down: u64,
    /// Number of connections.
    pub connections: u64,
}

/// Parsed log entry from dae.log representing a connection event.
#[derive(Debug, Clone)]
pub struct DaeLogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    /// Parsed connection info if this is a connection log line.
    pub connection: Option<ParsedConnection>,
}

/// A connection parsed from dae log output.
#[derive(Debug, Clone)]
pub struct ParsedConnection {
    pub action: ConnectionAction,
    pub source: String,
    pub destination: String,
    pub outbound: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionAction {
    /// New connection opened.
    Open,
    /// Connection closed.
    Close,
}

/// Parse a dae log line into a structured entry.
///
/// dae log format (typical):
/// `time="..." level=info msg="..."` or similar logrus-style output.
pub fn parse_dae_log_line(line: &str) -> Option<DaeLogEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Basic parsing: extract timestamp, level, message
    let timestamp = extract_field(line, "time=").unwrap_or_default();
    let level = extract_field(line, "level=").unwrap_or_else(|| "info".to_string());
    let message = extract_field(line, "msg=").unwrap_or_else(|| line.to_string());

    // Try to parse connection info from the message
    let connection = parse_connection_from_message(&message);

    Some(DaeLogEntry {
        timestamp,
        level,
        message,
        connection,
    })
}

fn extract_field(line: &str, prefix: &str) -> Option<String> {
    let start = line.find(prefix)? + prefix.len();
    let rest = &line[start..];

    if let Some(quoted) = rest.strip_prefix('"') {
        // Quoted value
        let end = quoted.find('"')? + 1;
        Some(rest[1..end].to_string())
    } else {
        // Unquoted value (until space)
        let end = rest.find(' ').unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn parse_connection_from_message(message: &str) -> Option<ParsedConnection> {
    // dae logs connections in various formats depending on version.
    // Common patterns:
    // "connection opened: src=... dst=... outbound=... proto=..."
    // "connection closed: ..."

    let action = if message.contains("opened") || message.contains("new connection") {
        ConnectionAction::Open
    } else if message.contains("closed") || message.contains("connection end") {
        ConnectionAction::Close
    } else {
        return None;
    };

    let source = extract_kv(message, "src")
        .or_else(|| extract_kv(message, "source"))
        .unwrap_or_default();
    let destination = extract_kv(message, "dst")
        .or_else(|| extract_kv(message, "dest"))
        .or_else(|| extract_kv(message, "destination"))
        .unwrap_or_default();
    let outbound = extract_kv(message, "outbound")
        .or_else(|| extract_kv(message, "node"))
        .unwrap_or_default();
    let protocol = extract_kv(message, "proto")
        .or_else(|| extract_kv(message, "protocol"))
        .unwrap_or_else(|| "tcp".to_string());

    if source.is_empty() && destination.is_empty() {
        return None;
    }

    Some(ParsedConnection {
        action,
        source,
        destination,
        outbound,
        protocol,
    })
}

fn extract_kv(message: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=");
    let start = message.find(&pattern)? + pattern.len();
    let rest = &message[start..];
    let end = rest.find(' ').unwrap_or(rest.len());
    let value = rest[..end].trim_matches('"');
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_log_line() {
        let line = r#"time="2026-01-01T00:00:00Z" level=info msg="dae started""#;
        let entry = parse_dae_log_line(line).unwrap();
        assert_eq!(entry.level, "info");
        assert!(entry.connection.is_none());
    }

    #[test]
    fn parses_connection_open() {
        let msg =
            "connection opened: src=192.168.1.100:54321 dst=1.1.1.1:443 outbound=proxy proto=tcp";
        let conn = parse_connection_from_message(msg).unwrap();
        assert_eq!(conn.action, ConnectionAction::Open);
        assert_eq!(conn.source, "192.168.1.100:54321");
        assert_eq!(conn.destination, "1.1.1.1:443");
        assert_eq!(conn.outbound, "proxy");
    }

    #[test]
    fn empty_line_returns_none() {
        assert!(parse_dae_log_line("").is_none());
        assert!(parse_dae_log_line("   ").is_none());
    }
}
