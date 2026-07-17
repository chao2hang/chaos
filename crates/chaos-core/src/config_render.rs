//! Minimal dae config rendering from imported nodes (MVP fixed template).

use std::collections::HashSet;

/// Node fields needed to render a dae `node { ... }` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeForConfig {
    pub id: String,
    pub name: String,
    pub link: String,
}

/// Render a minimal bootable-ish dae config with the given nodes.
///
/// Node keys are `node.<sanitized_name>` with uniqueness suffixes from id when needed.
/// Group `proxy` omits `filter` (MVP: all nodes).
pub fn render_minimal_dae_config(nodes: &[NodeForConfig]) -> String {
    let mut out = String::with_capacity(768 + nodes.len() * 64);
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
         \x20\x20upstream {\n\
         \x20\x20\x20\x20alidns: 'udp://dns.alidns.com:53'\n\
         \x20\x20\x20\x20googledns: 'tcp+udp://dns.google:53'\n\
         \x20\x20}\n\
         \x20\x20routing {\n\
         \x20\x20\x20\x20request {\n\
         \x20\x20\x20\x20\x20\x20fallback: alidns\n\
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

    out.push_str(
        "}\n\
         \n\
         group {\n\
         \x20\x20proxy {\n\
         \x20\x20\x20\x20policy: min_moving_avg\n\
         \x20\x20}\n\
         }\n\
         \n\
         routing {\n\
         \x20\x20pname(NetworkManager, systemd-resolved) -> must_direct\n\
         \x20\x20fallback: proxy\n\
         }\n",
    );

    out
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
}
