//! dae config parser — reverse of config_render.
//!
//! Parses a `.dae` configuration file back into structured data that can be
//! imported into the chaos database.

use serde::{Deserialize, Serialize};

/// Parsed result of a dae config file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedDaeConfig {
    pub nodes: Vec<ParsedNode>,
    pub groups: Vec<ParsedGroup>,
    pub routing_rules: Vec<ParsedRoutingRule>,
    pub routing_fallback: String,
    pub dns_upstreams: Vec<ParsedDnsUpstream>,
    pub dns_rules: Vec<ParsedDnsRule>,
    pub dns_fallback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedNode {
    pub name: String,
    pub link: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedGroup {
    pub name: String,
    pub policy: String,
    pub filter: Option<String>,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedRoutingRule {
    pub expression: String,
    pub outbound: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDnsUpstream {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDnsRule {
    pub expression: String,
    pub upstream: String,
}

/// Parse a dae config file content into structured data.
///
/// This is a best-effort parser that handles the common dae config format.
/// It does not validate the config — that's dae's job.
pub fn parse_dae_config(content: &str) -> ParsedDaeConfig {
    let mut config = ParsedDaeConfig::default();

    let mut current_section = Section::None;
    let mut current_subsection = SubSection::None;
    let mut current_group: Option<ParsedGroup> = None;
    let mut in_dns_routing_request = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Track section changes (only at top level)
        if current_section == Section::None {
            if trimmed == "global {" {
                current_section = Section::Global;
                continue;
            } else if trimmed == "dns {" {
                current_section = Section::Dns;
                continue;
            } else if trimmed == "subscription {" {
                current_section = Section::Subscription;
                continue;
            } else if trimmed == "node {" {
                current_section = Section::Node;
                continue;
            } else if trimmed == "group {" {
                current_section = Section::Group;
                continue;
            } else if trimmed == "routing {" {
                current_section = Section::Routing;
                continue;
            }
        }

        // Handle closing braces
        if trimmed == "}" {
            if let Some(group) = current_group.take() {
                config.groups.push(group);
            }
            if in_dns_routing_request {
                // Closing the `request {` block inside dns routing
                in_dns_routing_request = false;
            } else if current_subsection != SubSection::None {
                current_subsection = SubSection::None;
            } else {
                current_section = Section::None;
            }
            continue;
        }

        // Parse based on current section
        match current_section {
            Section::Node => {
                if let Some(node) = parse_node_line(trimmed) {
                    config.nodes.push(node);
                }
            }
            Section::Group => {
                parse_group_line(trimmed, &mut current_group);
            }
            Section::Routing => {
                if let Some(rule) = parse_routing_line(trimmed) {
                    config.routing_rules.push(rule);
                } else if let Some(fallback) = parse_fallback(trimmed) {
                    config.routing_fallback = fallback;
                }
            }
            Section::Dns => {
                if trimmed == "upstream {" {
                    current_subsection = SubSection::DnsUpstream;
                } else if trimmed == "routing {" {
                    current_subsection = SubSection::DnsRouting;
                } else if trimmed == "request {" {
                    in_dns_routing_request = true;
                } else {
                    match current_subsection {
                        SubSection::DnsUpstream => {
                            if let Some(upstream) = parse_dns_upstream_line(trimmed) {
                                config.dns_upstreams.push(upstream);
                            }
                        }
                        SubSection::DnsRouting if in_dns_routing_request => {
                            if let Some(rule) = parse_dns_rule_line(trimmed) {
                                config.dns_rules.push(rule);
                            } else if let Some(fallback) = parse_fallback(trimmed) {
                                config.dns_fallback = fallback;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // Push any remaining group
    if let Some(group) = current_group.take() {
        config.groups.push(group);
    }

    config
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Global,
    Dns,
    Subscription,
    Node,
    Group,
    Routing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubSection {
    None,
    DnsUpstream,
    DnsRouting,
}

/// Parse a node line like `node.name: 'link'`
fn parse_node_line(line: &str) -> Option<ParsedNode> {
    let line = line.strip_prefix("node.")?;
    let (name, rest) = line.split_once(':')?;
    let link = rest.trim().trim_matches('\'').trim_matches('"');
    if link.is_empty() {
        return None;
    }
    Some(ParsedNode {
        name: name.trim().to_string(),
        link: link.to_string(),
    })
}

/// Parse group section lines.
fn parse_group_line(line: &str, current_group: &mut Option<ParsedGroup>) {
    // Group name line: `groupname {`
    if line.ends_with('{') {
        let name = line.trim_end_matches('{').trim();
        *current_group = Some(ParsedGroup {
            name: name.to_string(),
            policy: "min_moving_avg".to_string(),
            filter: None,
            members: Vec::new(),
        });
        return;
    }

    let Some(group) = current_group.as_mut() else {
        return;
    };

    // filter: name(a, b, c) or filter: subtag(x)
    if let Some(filter) = line.strip_prefix("filter:") {
        let filter = filter.trim();
        group.filter = Some(filter.to_string());
        // Extract member names from name(...) filter
        if let Some(names) = extract_filter_names(filter) {
            group.members = names;
        }
    }
    // policy: xxx or policy: fixed(a, b)
    else if let Some(policy) = line.strip_prefix("policy:") {
        let policy = policy.trim();
        // Extract base policy name (before parentheses)
        let base = policy.split('(').next().unwrap_or(policy).trim();
        group.policy = base.to_string();
    }
}

/// Extract node names from a filter like `name(a, b, c)`
fn extract_filter_names(filter: &str) -> Option<Vec<String>> {
    let start = filter.find("name(")? + 5;
    let end = filter.rfind(')')?;
    if start >= end {
        return None;
    }
    let names_str = &filter[start..end];
    Some(
        names_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

/// Parse a routing rule line like `domain(suffix: example.com) -> proxy`
fn parse_routing_line(line: &str) -> Option<ParsedRoutingRule> {
    let (expression, outbound) = line.split_once("->")?;
    Some(ParsedRoutingRule {
        expression: expression.trim().to_string(),
        outbound: outbound.trim().to_string(),
    })
}

/// Parse a fallback line like `fallback: proxy`
fn parse_fallback(line: &str) -> Option<String> {
    let value = line.strip_prefix("fallback:")?;
    Some(value.trim().to_string())
}

/// Parse a DNS upstream line like `alidns: 'udp://dns.alidns.com:53'`
fn parse_dns_upstream_line(line: &str) -> Option<ParsedDnsUpstream> {
    let (name, rest) = line.split_once(':')?;
    let address = rest.trim().trim_matches('\'').trim_matches('"');
    if address.is_empty() {
        return None;
    }
    Some(ParsedDnsUpstream {
        name: name.trim().to_string(),
        address: address.to_string(),
    })
}

/// Parse a DNS rule line like `qname(geosite:cn) -> alidns`
fn parse_dns_rule_line(line: &str) -> Option<ParsedDnsRule> {
    let (expression, upstream) = line.split_once("->")?;
    Some(ParsedDnsRule {
        expression: expression.trim().to_string(),
        upstream: upstream.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let config = r#"
global {
  log_level: info
  tproxy_port: 12345
}

node {
  node.hk_01: 'trojan://x@1.1.1.1:443'
  node.us_01: 'ss://y@2.2.2.2:8388'
}

group {
  proxy {
    filter: name(hk_01, us_01)
    policy: min_moving_avg
  }
}

routing {
  pname(NetworkManager) -> must_direct
  fallback: proxy
}
"#;
        let parsed = parse_dae_config(config);
        assert_eq!(parsed.nodes.len(), 2);
        assert_eq!(parsed.nodes[0].name, "hk_01");
        assert_eq!(parsed.nodes[0].link, "trojan://x@1.1.1.1:443");
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].name, "proxy");
        assert_eq!(parsed.groups[0].members, vec!["hk_01", "us_01"]);
        assert_eq!(parsed.routing_rules.len(), 1);
        assert_eq!(parsed.routing_fallback, "proxy");
    }

    #[test]
    fn parses_dns_section() {
        let config = "dns {\n  upstream {\n    alidns: 'udp://dns.alidns.com:53'\n    googledns: 'tcp+udp://dns.google:53'\n  }\n  routing {\n    request {\n      qname(geosite:cn) -> alidns\n      fallback: googledns\n    }\n  }\n}\n";
        let parsed = parse_dae_config(config);
        assert_eq!(parsed.dns_upstreams.len(), 2);
        assert_eq!(parsed.dns_upstreams[0].name, "alidns");
        assert_eq!(
            parsed.dns_rules.len(),
            1,
            "dns_rules: {:?}",
            parsed.dns_rules
        );
        assert_eq!(parsed.dns_fallback, "googledns");
    }

    #[test]
    fn empty_config_returns_defaults() {
        let parsed = parse_dae_config("");
        assert!(parsed.nodes.is_empty());
        assert!(parsed.groups.is_empty());
        assert!(parsed.routing_rules.is_empty());
    }
}
