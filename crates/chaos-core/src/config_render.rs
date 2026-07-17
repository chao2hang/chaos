//! Minimal dae config rendering from imported nodes (MVP fixed template).

/// Node fields needed to render a dae `node { ... }` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeForConfig {
    pub id: String,
    pub name: String,
    pub link: String,
}

/// Render a minimal bootable-ish dae config with the given nodes.
///
/// Node keys are `node.<sanitized_name>`. Group `proxy` omits `filter` (MVP: all nodes).
/// Field names follow current dae config style (best-effort for the pinned dae version).
pub fn render_minimal_dae_config(nodes: &[NodeForConfig]) -> String {
    let mut out = String::with_capacity(512 + nodes.len() * 64);

    out.push_str(
        "global {\n\
         \x20\x20log_level: info\n\
         \x20\x20tproxy_port: 12345\n\
         \x20\x20allow_insecure: false\n\
         \x20\x20wan_interface: auto\n\
         \x20\x20auto_config_kernel_parameter: true\n\
         }\n\
         \n\
         subscription {\n\
         }\n\
         \n\
         node {\n",
    );

    for n in nodes {
        let key = format!("node.{}", sanitize_node_name(&n.name, &n.id));
        let link = escape_single_quotes(&n.link);
        out.push_str("  ");
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
            // map disallowed chars to '_', collapse runs
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
    // Keep single-quoted dae strings parseable.
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
        assert!(!s.contains("substring:"));
    }
}
