//! Subscription body decode and link extraction (MVP).

use crate::link::detect_protocol;

/// Schemes accepted from subscription feeds (after `detect_protocol` mapping).
fn is_known_protocol(protocol: &str) -> bool {
    matches!(
        protocol,
        "shadowsocks"
            | "shadowsocksr"
            | "vmess"
            | "vless"
            | "trojan"
            | "hysteria2"
            | "hysteria"
            | "tuic"
            | "http"
            | "https"
            | "socks"
            | "socks5"
    )
}

/// Decode subscription body: prefer valid UTF-8 text with share links;
/// otherwise try standard (and common variants of) base64, then UTF-8 of decoded bytes.
pub fn decode_subscription_body(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        if looks_like_link_list(s) {
            return s.to_string();
        }
        if let Some(decoded) = try_base64_to_utf8(s.trim()) {
            return decoded;
        }
        return s.to_string();
    }

    if let Some(decoded) = try_base64_bytes_to_utf8(bytes) {
        return decoded;
    }

    String::from_utf8_lossy(bytes).into_owned()
}

fn looks_like_link_list(s: &str) -> bool {
    s.lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && detect_protocol(line).is_some())
}

fn try_base64_to_utf8(input: &str) -> Option<String> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return None;
    }
    try_base64_bytes_to_utf8(cleaned.as_bytes())
}

fn try_base64_bytes_to_utf8(bytes: &[u8]) -> Option<String> {
    use base64::Engine;
    let engines = [
        base64::engine::general_purpose::STANDARD,
        base64::engine::general_purpose::STANDARD_NO_PAD,
        base64::engine::general_purpose::URL_SAFE,
        base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ];
    for eng in engines {
        if let Ok(raw) = eng.decode(bytes) {
            if let Ok(s) = String::from_utf8(raw) {
                return Some(s);
            }
        }
    }
    None
}

/// Split body into lines and keep only known proxy share links.
pub fn parse_subscription_links(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| {
            detect_protocol(line)
                .map(|p| is_known_protocol(&p))
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
        .collect()
}

/// Redact the secret-bearing part of a subscription URL for logging.
///
/// Subscription URLs carry their access token in the path (providers commonly
/// use `https://host/<token>/<token>`), so the full URL must never reach the
/// logs or an issue report. Keep scheme, host and port — enough to tell which
/// provider failed — and replace authority userinfo and the rest of the URL.
pub fn redact_subscription_url_for_log(url: &str) -> String {
    const PLACEHOLDER: &str = "<redacted-url>";

    let Some(scheme_end) = url.find("://") else {
        return PLACEHOLDER.to_string();
    };
    let after_scheme = &url[scheme_end + 3..];

    // The authority ends at the first path, query or fragment delimiter.
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    // Drop any `user:password@` userinfo.
    let host = authority.rsplit('@').next().unwrap_or(authority);

    if host.is_empty() {
        return PLACEHOLDER.to_string();
    }
    format!("{}://{host}/<redacted>", &url[..scheme_end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn decodes_plain_utf8_links() {
        let body = "trojan://example@1.2.3.4:443\nvmess://abc\n";
        assert_eq!(decode_subscription_body(body.as_bytes()), body);
    }

    #[test]
    fn decodes_standard_base64_body() {
        let plain = "trojan://example@1.2.3.4:443\nss://YWFh\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(plain.as_bytes());
        let decoded = decode_subscription_body(b64.as_bytes());
        assert_eq!(decoded, plain);
    }

    #[test]
    fn parse_filters_known_schemes_and_comments() {
        let body = r#"
# comment
trojan://u@1.2.3.4:443
not-a-link
custom://skip-me
vmess://payload
ss://base64payload
"#;
        let links = parse_subscription_links(body);
        assert_eq!(links.len(), 3);
        assert!(links[0].starts_with("trojan://"));
        assert!(links[1].starts_with("vmess://"));
        assert!(links[2].starts_with("ss://"));
    }

    #[test]
    fn plain_utf8_without_links_still_returned() {
        let body = "hello world";
        assert_eq!(decode_subscription_body(body.as_bytes()), body);
    }

    #[test]
    fn redacts_subscription_token_path() {
        let url = "https://dy11.example.com/ckt3kzjcmfcbx300/dztkbmehjv2pw869/0bc4b2d92ccd6777";
        let redacted = redact_subscription_url_for_log(url);
        assert_eq!(redacted, "https://dy11.example.com/<redacted>");
        assert!(!redacted.contains("ckt3kzjcmfcbx300"));
        assert!(!redacted.contains("0bc4b2d92ccd6777"));
    }

    #[test]
    fn redacts_query_and_userinfo_but_keeps_host_and_port() {
        let redacted =
            redact_subscription_url_for_log("https://user:pw@sub.example.com:8443/tok?token=abc");
        assert_eq!(redacted, "https://sub.example.com:8443/<redacted>");
        assert!(!redacted.contains("token=abc"));
        assert!(!redacted.contains("pw"));
    }

    #[test]
    fn redacts_bare_host_and_rejects_non_urls() {
        assert_eq!(
            redact_subscription_url_for_log("http://example.com"),
            "http://example.com/<redacted>"
        );
        assert_eq!(
            redact_subscription_url_for_log("not-a-url"),
            "<redacted-url>"
        );
        assert_eq!(
            redact_subscription_url_for_log("https://"),
            "<redacted-url>"
        );
    }
}
