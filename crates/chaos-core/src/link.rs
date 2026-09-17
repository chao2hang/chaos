//! Share-link parsing helpers (MVP best-effort).

/// Detect protocol from a share link scheme.
///
/// Maps common proxy schemes to stable names; unknown schemes are returned as-is.
/// Returns `None` when the input has no `scheme://` prefix.
pub fn detect_protocol(link: &str) -> Option<String> {
    let link = link.trim();
    let (scheme, _) = link.split_once("://")?;
    if scheme.is_empty() {
        return None;
    }
    let scheme = scheme.to_ascii_lowercase();
    Some(match scheme.as_str() {
        "ss" => "shadowsocks".to_string(),
        "ssr" => "shadowsocksr".to_string(),
        "hy2" | "hysteria2" => "hysteria2".to_string(),
        other => other.to_string(),
    })
}

/// Whether the protocol only uses UDP as its transport.
///
/// Nodes for these protocols cannot be health-checked with a plain TCP
/// `connect()`: their server may not listen on the corresponding TCP port at
/// all (for example, hysteria2/QUIC is UDP-only). The latency prober uses this
/// to skip a TCP fallback that would otherwise report a healthy node as dead.
pub fn is_udp_only_protocol(protocol: &str) -> bool {
    matches!(
        protocol.to_ascii_lowercase().as_str(),
        "hysteria2" | "hysteria" | "tuic"
    )
}

/// Stable code for a hysteria2 link carrying `obfs` parameters that the
/// bundled dae data plane silently drops.
pub const WARN_HYSTERIA2_OBFS_UNSUPPORTED: &str = "hysteria2_obfs_unsupported";

/// Inspect a share link for parameters the bundled dae silently drops.
///
/// The vendored dae (olicesx/outbound fork, as bundled in dae v2.0.0) does not
/// implement hysteria2 salamander obfuscation — its dialer still carries a
/// `TODO: support salamander obfuscation`. `obfs` / `obfs-password` are ignored
/// and the resulting bare-QUIC dialer can never reach a server that enables the
/// plugin, so the node appears importable yet times out forever with no hint.
/// Callers (import / node edit) should surface the returned codes as explicit
/// warnings instead of letting the parameters vanish silently.
///
/// Returns stable warning codes (see the `WARN_*` constants).
pub fn link_compatibility_warnings(link: &str) -> Vec<&'static str> {
    let link = link.trim();
    let Some((scheme, rest)) = link.split_once("://") else {
        return Vec::new();
    };
    if scheme.is_empty() {
        return Vec::new();
    }
    if !matches!(scheme.to_ascii_lowercase().as_str(), "hysteria2" | "hy2") {
        return Vec::new();
    }

    // Query sits between the first '?' and the fragment ('#').
    let before_fragment = rest.split('#').next().unwrap_or(rest);
    let Some(query) = before_fragment.split_once('?').map(|(_, query)| query) else {
        return Vec::new();
    };
    let has_key = |name: &str| {
        query.split('&').any(|pair| {
            let (key, _) = pair.split_once('=').unwrap_or((pair, ""));
            key == name
        })
    };
    if has_key("obfs") || has_key("obfs-password") {
        vec![WARN_HYSTERIA2_OBFS_UNSUPPORTED]
    } else {
        Vec::new()
    }
}

/// Best-effort host:port (or host) extraction from a URL-ish share link.
pub fn detect_address(link: &str) -> Option<String> {
    let link = link.trim();
    let rest = link.split_once("://")?.1;
    if rest.is_empty() {
        return None;
    }

    // Drop query/fragment; authority is before first path segment.
    let before_q = rest.split(['?', '#']).next().unwrap_or(rest);
    let authority = before_q.split('/').next().unwrap_or(before_q);
    if authority.is_empty() {
        return None;
    }

    // userinfo@host:port
    let hostport = match authority.rsplit_once('@') {
        Some((_, hp)) => hp,
        None => authority,
    };
    if hostport.is_empty() {
        return None;
    }
    Some(hostport.to_string())
}

/// Extract per-link tag from the URL fragment (`#...`), URL-decoded.
///
/// E.g. `vless://...#%E9%A6%99%E6%B8%AF01` → `Some("香港01")`.
pub fn detect_tag(link: &str) -> Option<String> {
    let fragment = link.trim().rsplit_once('#')?.1;
    if fragment.is_empty() {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(fragment)
        .decode_utf8()
        .ok()?;
    let tag = decoded.trim();
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
}

/// Display name for a node.
///
/// Priority: per-link tag (from `#fragment`) → subscription prefix + tag
/// → subscription prefix + protocol-shortid → protocol-shortid.
pub fn node_name(
    link_tag: Option<&str>,
    sub_tag: Option<&str>,
    protocol: Option<&str>,
    id: &str,
) -> String {
    let link_tag = link_tag.map(str::trim).filter(|t| !t.is_empty());
    let sub_tag = sub_tag.map(str::trim).filter(|t| !t.is_empty());

    match (link_tag, sub_tag) {
        (Some(lt), Some(st)) => format!("{st} · {lt}"),
        (Some(lt), None) => lt.to_string(),
        (None, Some(st)) => {
            let proto = protocol.unwrap_or("node");
            let short = short_id_prefix(id);
            format!("{st}-{proto}-{short}")
        }
        (None, None) => {
            let proto = protocol.unwrap_or("node");
            let short = short_id_prefix(id);
            format!("{proto}-{short}")
        }
    }
}

fn short_id_prefix(id: &str) -> &str {
    let end = id.char_indices().nth(8).map(|(i, _)| i).unwrap_or(id.len());
    &id[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ss_scheme() {
        assert_eq!(detect_protocol("ss://aaa").as_deref(), Some("shadowsocks"));
    }

    #[test]
    fn detects_known_and_unknown_schemes() {
        assert_eq!(detect_protocol("ssr://x").as_deref(), Some("shadowsocksr"));
        assert_eq!(detect_protocol("vmess://x").as_deref(), Some("vmess"));
        assert_eq!(detect_protocol("vless://x").as_deref(), Some("vless"));
        assert_eq!(detect_protocol("trojan://x").as_deref(), Some("trojan"));
        assert_eq!(
            detect_protocol("hysteria2://x").as_deref(),
            Some("hysteria2")
        );
        assert_eq!(detect_protocol("hy2://x").as_deref(), Some("hysteria2"));
        assert_eq!(detect_protocol("custom://x").as_deref(), Some("custom"));
        assert_eq!(detect_protocol("SS://AAA").as_deref(), Some("shadowsocks"));
        assert_eq!(detect_protocol("not-a-link"), None);
        assert_eq!(detect_protocol(""), None);
        assert_eq!(detect_protocol("://missing"), None);
    }

    #[test]
    fn detects_udp_only_protocols() {
        assert!(is_udp_only_protocol("hysteria2"));
        assert!(is_udp_only_protocol("HYSTERIA"));
        assert!(is_udp_only_protocol("tuic"));
        assert!(!is_udp_only_protocol("trojan"));
        assert!(!is_udp_only_protocol("shadowsocks"));
    }

    #[test]
    fn detects_address_from_userinfo_url() {
        assert_eq!(
            detect_address("trojan://example@1.2.3.4:443?sni=x#test").as_deref(),
            Some("1.2.3.4:443")
        );
        assert_eq!(
            detect_address("vless://1.2.3.4:443").as_deref(),
            Some("1.2.3.4:443")
        );
        assert_eq!(
            detect_address("ss://onlybase64").as_deref(),
            Some("onlybase64")
        );
        assert_eq!(detect_address("nope"), None);
    }

    #[test]
    fn node_name_prefers_link_tag() {
        // link_tag + sub_tag → "sub · link"
        assert_eq!(
            node_name(Some("HK01"), Some("顶级"), Some("trojan"), "abcdef12-xxxx"),
            "顶级 · HK01"
        );
        // link_tag only → link_tag
        assert_eq!(
            node_name(Some("HK01"), None, Some("trojan"), "abcdef12-xxxx"),
            "HK01"
        );
        // sub_tag only → "sub-proto-short"
        assert_eq!(
            node_name(None, Some("顶级"), Some("trojan"), "abcdef12-xxxx"),
            "顶级-trojan-abcdef12"
        );
        // neither → "proto-short"
        assert_eq!(
            node_name(None, None, Some("trojan"), "abcdef12-xxxx"),
            "trojan-abcdef12"
        );
        assert_eq!(node_name(Some("  "), None, None, "short"), "node-short");
    }

    #[test]
    fn detects_tag_from_fragment() {
        assert_eq!(
            detect_tag("vless://user@1.2.3.4:443?sni=x#HK-01").as_deref(),
            Some("HK-01")
        );
        // URL-encoded Chinese
        assert_eq!(
            detect_tag("trojan://u@1.2.3.4:443#%E9%A6%99%E6%B8%AF01").as_deref(),
            Some("香港01")
        );
        // No fragment
        assert_eq!(detect_tag("vless://u@1.2.3.4:443"), None);
        // Empty fragment
        assert_eq!(detect_tag("vless://u@1.2.3.4:443#"), None);
        // Whitespace-only fragment
        assert_eq!(detect_tag("vless://u@1.2.3.4:443#%20%20"), None);
    }

    #[test]
    fn warns_on_hysteria2_obfs_params() {
        assert_eq!(
            link_compatibility_warnings(
                "hysteria2://pass@h:8443/?obfs=salamander&obfs-password=pw&insecure=1#n"
            ),
            vec![WARN_HYSTERIA2_OBFS_UNSUPPORTED]
        );
        // `hy2` alias and `obfs` alone also warn.
        assert_eq!(
            link_compatibility_warnings("hy2://pass@h:8443/?obfs=salamander#n"),
            vec![WARN_HYSTERIA2_OBFS_UNSUPPORTED]
        );
        assert_eq!(
            link_compatibility_warnings("hysteria2://pass@h:8443/?obfs-password=pw#n"),
            vec![WARN_HYSTERIA2_OBFS_UNSUPPORTED]
        );
        // Plain hysteria2 and other protocols are silent.
        assert!(
            link_compatibility_warnings("hysteria2://pass@h:8443/?insecure=1&sni=h#n").is_empty()
        );
        assert!(link_compatibility_warnings("hysteria://pass@h:8443/?obfs=x#n").is_empty());
        assert!(link_compatibility_warnings("trojan://u@h:443?obfs=x#n").is_empty());
        assert!(link_compatibility_warnings("not-a-link").is_empty());
        // Range-port links work on the bundled dae; no warning for them.
        assert!(
            link_compatibility_warnings("hysteria2://pass@h:30000-30049/?insecure=1#n").is_empty()
        );
    }
}
