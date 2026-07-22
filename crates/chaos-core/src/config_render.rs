//! dae config rendering from nodes + groups + routing + DNS.

use std::collections::HashSet;

pub const MAX_DAE_IDENTIFIER_LENGTH: usize = 128;

/// LAN gateway mode configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LanConfig {
    /// Enable LAN gateway mode (allow LAN devices to use this proxy).
    pub enabled: bool,
    /// LAN interface name (e.g., "eth0", "br-lan").
    pub lan_interface: Option<String>,
    /// Disable SNAT (for advanced routing setups).
    pub disable_snat: bool,
}

/// Node fields needed to render a dae `node { ... }` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeForConfig {
    pub id: String,
    pub name: String,
    pub link: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMemberForConfig {
    /// Original node id from store; mapped to rendered dae key at render time.
    pub node_id: String,
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupForConfig {
    pub name: String,
    pub policy: String,
    /// When set, emits `filter: subtag(tag)` style filter (legacy).
    pub filter_tag: Option<String>,
    /// Explicit members: when non-empty, emit `filter: name(node.a, node.b)` and
    /// `policy: fixed(node.a, node.b)` (names match the `node.<key>:` dialer tags).
    pub members: Vec<GroupMemberForConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingRuleForConfig {
    pub expression: String,
    pub outbound: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsUpstreamForConfig {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsRuleForConfig {
    pub expression: String,
    pub upstream: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPlane {
    pub groups: Vec<GroupForConfig>,
    pub routing_rules: Vec<RoutingRuleForConfig>,
    pub routing_fallback: String,
    pub dns_upstreams: Vec<DnsUpstreamForConfig>,
    pub dns_rules: Vec<DnsRuleForConfig>,
    pub dns_fallback: String,
}

impl Default for ConfigPlane {
    fn default() -> Self {
        Self {
            groups: vec![GroupForConfig {
                name: "proxy".into(),
                policy: "min_moving_avg".into(),
                filter_tag: None,
                members: vec![],
            }],
            routing_rules: vec![RoutingRuleForConfig {
                expression: "pname(NetworkManager, systemd-resolved)".into(),
                outbound: "must_direct".into(),
                enabled: true,
            }],
            routing_fallback: "proxy".into(),
            dns_upstreams: vec![
                DnsUpstreamForConfig {
                    name: "alidns".into(),
                    address: "udp://dns.alidns.com:53".into(),
                },
                DnsUpstreamForConfig {
                    name: "googledns".into(),
                    address: "tcp+udp://dns.google:53".into(),
                },
            ],
            dns_rules: vec![],
            dns_fallback: "alidns".into(),
        }
    }
}

/// Render a full dae config from nodes + config plane.
pub fn render_dae_config(nodes: &[NodeForConfig], plane: &ConfigPlane) -> String {
    render_dae_config_with_lan(nodes, plane, None)
}

/// Render a full dae config with optional LAN gateway mode.
pub fn render_dae_config_with_lan(
    nodes: &[NodeForConfig],
    plane: &ConfigPlane,
    lan_config: Option<&LanConfig>,
) -> String {
    let mut out = String::with_capacity(1024 + nodes.len() * 64);
    let mut used_keys: HashSet<String> = HashSet::new();

    // Global section
    out.push_str("global {\n");
    out.push_str("  log_level: info\n");
    out.push_str("  tproxy_port: 12345\n");
    out.push_str("  allow_insecure: false\n");
    out.push_str("  wan_interface: auto\n");
    out.push_str("  auto_config_kernel_parameter: true\n");

    // LAN gateway mode configuration
    if let Some(lan) = lan_config {
        if lan.enabled {
            if let Some(ref iface) = lan.lan_interface {
                out.push_str(&format!("  lan_interface: {}\n", iface));
            }
            if lan.disable_snat {
                out.push_str("  disable_snat: true\n");
            }
        }
    }
    out.push_str("}\n\n");

    // DNS section
    out.push_str("dns {\n  upstream {\n");

    for u in &plane.dns_upstreams {
        let Some(name) = normalized_dae_identifier(&u.name) else {
            continue;
        };
        out.push_str("    ");
        out.push_str(&name);
        out.push_str(": '");
        out.push_str(&escape_single_quotes(&u.address));
        out.push_str("'\n");
    }

    out.push_str(
        "  }\n\
         \x20\x20routing {\n\
         \x20\x20\x20\x20request {\n",
    );
    for r in plane.dns_rules.iter().filter(|r| r.enabled) {
        if r.expression.trim().is_empty() {
            continue;
        }
        out.push_str("      ");
        out.push_str(r.expression.trim());
        out.push_str(" -> ");
        out.push_str(
            normalized_dae_identifier(&r.upstream)
                .as_deref()
                .unwrap_or("__chaos_invalid_dns__"),
        );
        out.push('\n');
    }
    out.push_str("      fallback: ");
    out.push_str(
        normalized_dae_identifier(&plane.dns_fallback)
            .as_deref()
            .unwrap_or("__chaos_invalid_dns__"),
    );
    out.push_str(
        "\n\
         \x20\x20\x20\x20}\n\
         \x20\x20}\n\
         }\n\
         \n\
         subscription {\n\
         }\n\
         \n\
         node {\n",
    );

    for n in nodes {
        let key = unique_node_key(&n.name, &n.id, &mut used_keys);
        let link = escape_single_quotes(&n.link);
        out.push_str("  node.");
        out.push_str(&key);
        out.push_str(": '");
        out.push_str(&link);
        out.push_str("'\n");
    }

    // Map node id -> rendered dialer name used by group filters.
    // Node lines are emitted as `node.<key>: 'link'`. dae stores that as
    // KeyableString `node.<key>:link`, so Property.Name becomes `node.<key>`.
    // `filter: name(...)` must match that full name, not the bare `<key>`.
    let mut id_to_filter_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    {
        let mut used_keys2: HashSet<String> = HashSet::new();
        for n in nodes {
            let key = unique_node_key(&n.name, &n.id, &mut used_keys2);
            id_to_filter_name.insert(n.id.clone(), format!("node.{key}"));
        }
    }

    out.push_str("}\n\ngroup {\n");
    for g in &plane.groups {
        let Some(gname) = normalized_dae_identifier(&g.name) else {
            continue;
        };
        out.push_str("  ");
        out.push_str(&gname);
        out.push_str(" {\n");

        // Prefer explicit members; fall back to subtag filter.
        let member_names: Vec<(String, u32)> = g
            .members
            .iter()
            .filter_map(|m| {
                id_to_filter_name
                    .get(&m.node_id)
                    .cloned()
                    .map(|name| (name, m.weight.max(1)))
            })
            .collect();

        if !member_names.is_empty() {
            out.push_str("    filter: name(");
            for (i, (name, _)) in member_names.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(name);
            }
            out.push_str(")\n");
            let policy = g.policy.trim();
            // Weighted fixed/random: repeat dialer names by weight → fixed(a, a, b).
            if policy == "fixed" || policy == "random" {
                out.push_str("    policy: ");
                out.push_str(policy);
                out.push('(');
                let mut first = true;
                for (name, w) in &member_names {
                    for _ in 0..*w {
                        if !first {
                            out.push_str(", ");
                        }
                        first = false;
                        out.push_str(name);
                    }
                }
                out.push_str(")\n");
            } else {
                out.push_str("    policy: ");
                out.push_str(policy);
                out.push('\n');
            }
        } else {
            if !g.members.is_empty() {
                // Explicit groups must fail closed. Missing member ids must never
                // degrade into an unfiltered group that can select every node.
                out.push_str("    filter: name(__chaos_missing_member__)\n");
            }
            if let Some(tag) = g
                .filter_tag
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                out.push_str("    filter: subtag(");
                out.push_str(&sanitize_ident(tag));
                out.push_str(")\n");
            }
            out.push_str("    policy: ");
            out.push_str(g.policy.trim());
            out.push('\n');
        }
        out.push_str("  }\n");
    }
    out.push_str("}\n\nrouting {\n");
    for r in plane.routing_rules.iter().filter(|r| r.enabled) {
        if r.expression.trim().is_empty() {
            continue;
        }
        out.push_str("  ");
        out.push_str(r.expression.trim());
        out.push_str(" -> ");
        out.push_str(
            normalized_dae_identifier(&r.outbound)
                .as_deref()
                .unwrap_or("direct"),
        );
        out.push('\n');
    }
    out.push_str("  fallback: ");
    out.push_str(
        normalized_dae_identifier(&plane.routing_fallback)
            .as_deref()
            .unwrap_or("direct"),
    );
    out.push_str("\n}\n");

    out
}

/// Backward-compatible helper: nodes only + default plane.
pub fn render_minimal_dae_config(nodes: &[NodeForConfig]) -> String {
    render_dae_config(nodes, &ConfigPlane::default())
}

fn unique_node_key(name: &str, id: &str, used: &mut HashSet<String>) -> String {
    let base = sanitize_node_name(name, id);
    if used.insert(base.clone()) {
        return base;
    }
    let suffix = short_id_suffix(id);
    let mut candidate = format!("{base}_{suffix}");
    let mut n = 2u32;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}_{suffix}_{n}");
        n += 1;
    }
    candidate
}

fn short_id_suffix(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    if cleaned.is_empty() {
        "x".into()
    } else {
        cleaned
    }
}

/// Keep only `[A-Za-z0-9_]`; empty results fall back to sanitized `id` or `node`.
fn sanitize_node_name(name: &str, id: &str) -> String {
    let from_name = sanitize_ident(name);
    if !from_name.is_empty() {
        return from_name;
    }
    let from_id = sanitize_ident(id);
    if !from_id.is_empty() {
        return from_id;
    }
    "node".to_string()
}

/// Render a dae identifier consistently for group definitions and routing targets.
///
/// The renderer keeps ASCII letters and digits and folds every other character
/// into a single underscore. Callers that need a stable outbound name should use
/// this helper rather than reproducing the transformation.
pub fn dae_identifier(s: &str) -> String {
    sanitize_ident(s)
}

/// Return the stable dae identifier for a user-provided name when it is safe
/// to use as a config key. The caller may still preserve the original label for
/// display, but runtime references should use this normalized value.
pub fn normalized_dae_identifier(s: &str) -> Option<String> {
    let identifier = dae_identifier(s);
    if identifier.is_empty() || identifier.len() > MAX_DAE_IDENTIFIER_LENGTH {
        None
    } else {
        Some(identifier)
    }
}

pub fn is_reserved_dae_identifier(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "direct" | "must_direct" | "block"
    )
}

fn sanitize_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_us = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    while out.starts_with('_') {
        out.remove(0);
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        out.insert_str(0, "id_");
    }
    out
}

fn escape_single_quotes(s: &str) -> String {
    s.replace(['\r', '\n'], " ").replace('\'', "%27")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_node_link_as_node_line() {
        let s = render_minimal_dae_config(&[NodeForConfig {
            id: "a".into(),
            name: "n1".into(),
            link: "trojan://x@1.1.1.1:443".into(),
        }]);
        assert!(s.contains("node.n1"));
        assert!(s.contains("trojan://"));
        assert!(s.contains("routing {"));
        assert!(s.contains("dns {"));
        assert!(!s.contains("substring:"));
        assert!(!s.contains("filter:"));
    }

    #[test]
    fn sanitizes_node_name_for_keys() {
        let s = render_minimal_dae_config(&[NodeForConfig {
            id: "id1".into(),
            name: "HK-01 / vip!".into(),
            link: "ss://payload".into(),
        }]);
        assert!(s.contains("node.HK_01_vip"));
        assert!(!s.contains("HK-01"));
        assert!(!s.contains("vip!"));
    }

    #[test]
    fn unique_keys_when_names_collide() {
        let s = render_minimal_dae_config(&[
            NodeForConfig {
                id: "aaaa1111".into(),
                name: "same".into(),
                link: "trojan://a@1.1.1.1:443".into(),
            },
            NodeForConfig {
                id: "bbbb2222".into(),
                name: "same".into(),
                link: "trojan://b@2.2.2.2:443".into(),
            },
        ]);
        assert!(s.contains("node.same:"));
        assert!(s.contains("node.same_bbbb2222") || s.contains("node.same_bbbb"));
        let count_same = s.matches("node.same").count();
        assert!(count_same >= 2);
    }

    #[test]
    fn custom_plane_emits_group_filter_and_rules() {
        let nodes = [NodeForConfig {
            id: "n1".into(),
            name: "hk".into(),
            link: "trojan://x@1.1.1.1:443".into(),
        }];
        let plane = ConfigPlane {
            groups: vec![GroupForConfig {
                name: "home".into(),
                policy: "fixed".into(),
                filter_tag: None,
                members: vec![GroupMemberForConfig {
                    node_id: "n1".into(),
                    weight: 2,
                }],
            }],
            routing_rules: vec![RoutingRuleForConfig {
                expression: "domain(example.com)".into(),
                outbound: "home".into(),
                enabled: true,
            }],
            routing_fallback: "direct".into(),
            dns_upstreams: vec![DnsUpstreamForConfig {
                name: "cloudflare".into(),
                address: "udp://1.1.1.1:53".into(),
            }],
            dns_rules: vec![DnsRuleForConfig {
                expression: "qname(geosite:cn)".into(),
                upstream: "cloudflare".into(),
                enabled: true,
            }],
            dns_fallback: "cloudflare".into(),
        };
        let s = render_dae_config(&nodes, &plane);
        assert!(s.contains("home {"));
        // Node is declared as `node.hk:`; filter/policy must use the same dialer name.
        assert!(s.contains("node.hk:"));
        assert!(s.contains("filter: name(node.hk)"));
        assert!(s.contains("policy: fixed(node.hk, node.hk)"));
        assert!(s.contains("domain(example.com) -> home"));
        assert!(s.contains("fallback: direct"));
        assert!(s.contains("cloudflare: 'udp://1.1.1.1:53'"));
        assert!(s.contains("qname(geosite:cn) -> cloudflare"));
    }

    #[test]
    fn normalizes_and_limits_runtime_identifiers() {
        assert_eq!(
            normalized_dae_identifier("123 proxy"),
            Some("id_123_proxy".into())
        );
        assert_eq!(
            normalized_dae_identifier("proxy-name"),
            Some("proxy_name".into())
        );
        assert_eq!(normalized_dae_identifier("\u{8282}\u{70b9}"), None);
        assert_eq!(normalized_dae_identifier(&"a".repeat(129)), None);
        assert!(is_reserved_dae_identifier("DIRECT"));
    }

    #[test]
    fn normalizes_dns_references_and_removes_address_newlines() {
        let plane = ConfigPlane {
            groups: vec![],
            routing_rules: vec![],
            routing_fallback: "direct".into(),
            dns_upstreams: vec![DnsUpstreamForConfig {
                name: "cloud-flare".into(),
                address: "udp://1.1.1.1:53\ninjected".into(),
            }],
            dns_rules: vec![DnsRuleForConfig {
                expression: "qname(example.com)".into(),
                upstream: "cloud-flare".into(),
                enabled: true,
            }],
            dns_fallback: "cloud-flare".into(),
        };
        let rendered = render_dae_config(&[], &plane);
        assert!(rendered.contains("cloud_flare: 'udp://1.1.1.1:53 injected'"));
        assert!(rendered.contains("qname(example.com) -> cloud_flare"));
        assert!(rendered.contains("fallback: cloud_flare"));
    }
}
