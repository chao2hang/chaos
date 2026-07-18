//! dae config rendering from nodes + groups + routing + DNS.

use std::collections::HashSet;

/// Node fields needed to render a dae `node { ... }` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeForConfig {
    pub id: String,
    pub name: String,
    pub link: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupForConfig {
    pub name: String,
    pub policy: String,
    /// When set, emits `filter: subtag(tag)` style filter.
    pub filter_tag: Option<String>,
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
    let mut out = String::with_capacity(1024 + nodes.len() * 64);
    let mut used_keys: HashSet<String> = HashSet::new();

    out.push_str(
        "global {\n\
         \x20\x20log_level: info\n\
         \x20\x20tproxy_port: 12345\n\
         \x20\x20allow_insecure: false\n\
         \x20\x20wan_interface: auto\n\
         \x20\x20auto_config_kernel_parameter: true\n\
         }\n\
         \n\
         dns {\n\
         \x20\x20upstream {\n",
    );

    for u in &plane.dns_upstreams {
        let name = sanitize_ident(&u.name);
        if name.is_empty() {
            continue;
        }
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
        out.push_str(r.upstream.trim());
        out.push('\n');
    }
    out.push_str("      fallback: ");
    out.push_str(plane.dns_fallback.trim());
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

    out.push_str("}\n\ngroup {\n");
    let groups = if plane.groups.is_empty() {
        ConfigPlane::default().groups
    } else {
        plane.groups.clone()
    };
    for g in &groups {
        let gname = sanitize_ident(&g.name);
        if gname.is_empty() {
            continue;
        }
        out.push_str("  ");
        out.push_str(&gname);
        out.push_str(" {\n");
        if let Some(tag) = g.filter_tag.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            out.push_str("    filter: subtag(");
            out.push_str(tag);
            out.push_str(")\n");
        }
        out.push_str("    policy: ");
        out.push_str(g.policy.trim());
        out.push_str("\n  }\n");
    }
    out.push_str("}\n\nrouting {\n");
    for r in plane.routing_rules.iter().filter(|r| r.enabled) {
        if r.expression.trim().is_empty() {
            continue;
        }
        out.push_str("  ");
        out.push_str(r.expression.trim());
        out.push_str(" -> ");
        out.push_str(r.outbound.trim());
        out.push('\n');
    }
    out.push_str("  fallback: ");
    out.push_str(plane.routing_fallback.trim());
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

fn sanitize_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_us = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_us = false;
        } else if c == '_' || !last_us {
            if !last_us {
                out.push('_');
                last_us = true;
            }
        }
    }
    while out.starts_with('_') {
        out.remove(0);
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "%27")
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
        let plane = ConfigPlane {
            groups: vec![GroupForConfig {
                name: "home".into(),
                policy: "fixed".into(),
                filter_tag: Some("hk".into()),
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
        let s = render_dae_config(&[], &plane);
        assert!(s.contains("home {"));
        assert!(s.contains("filter: subtag(hk)"));
        assert!(s.contains("policy: fixed"));
        assert!(s.contains("domain(example.com) -> home"));
        assert!(s.contains("fallback: direct"));
        assert!(s.contains("cloudflare: 'udp://1.1.1.1:53'"));
        assert!(s.contains("qname(geosite:cn) -> cloudflare"));
    }
}
