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

/// MVP display name: non-empty tag wins; else `{protocol}-{short_id}`.
pub fn node_name(tag: Option<&str>, protocol: Option<&str>, id: &str) -> String {
    if let Some(tag) = tag.map(str::trim).filter(|t| !t.is_empty()) {
        return tag.to_string();
    }
    let proto = protocol.unwrap_or("node");
    let short = short_id_prefix(id);
    format!("{proto}-{short}")
}

fn short_id_prefix(id: &str) -> &str {
    let end = id
        .char_indices()
        .nth(8)
        .map(|(i, _)| i)
        .unwrap_or(id.len());
    &id[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ss_scheme() {
        assert_eq!(
            detect_protocol("ss://aaa").as_deref(),
            Some("shadowsocks")
        );
    }

    #[test]
    fn detects_known_and_unknown_schemes() {
        assert_eq!(
            detect_protocol("ssr://x").as_deref(),
            Some("shadowsocksr")
        );
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
    fn detects_address_from_userinfo_url() {
        assert_eq!(
            detect_address("trojan://example@1.2.3.4:443?sni=x#test").as_deref(),
            Some("1.2.3.4:443")
        );
        assert_eq!(
            detect_address("vless://1.2.3.4:443").as_deref(),
            Some("1.2.3.4:443")
        );
        assert_eq!(detect_address("ss://onlybase64").as_deref(), Some("onlybase64"));
        assert_eq!(detect_address("nope"), None);
    }

    #[test]
    fn node_name_prefers_tag() {
        assert_eq!(node_name(Some("n1"), Some("trojan"), "abcdef12-xxxx"), "n1");
        assert_eq!(
            node_name(None, Some("trojan"), "abcdef12-xxxx"),
            "trojan-abcdef12"
        );
        assert_eq!(node_name(Some("  "), None, "short"), "node-short");
    }
}
