//! Persisted orchestration graph (V4), validation, and dae routing compilation.

use std::collections::{HashMap, HashSet};

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use crate::config_render::{dae_identifier, is_reserved_dae_identifier, normalized_dae_identifier};

pub const ORCHESTRATION_VERSION: u32 = 4;
const MAX_SOURCE_WEIGHT: u32 = 99;
const MAX_RULE_PRIORITY: u32 = 9_999;
const GROUP_POLICIES: [&str; 4] = ["min_moving_avg", "min", "random", "fixed"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestrationDocument {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub nodes: Vec<FlowNode>,
    #[serde(default)]
    pub edges: Vec<FlowEdge>,
    #[serde(default)]
    pub viewport: FlowViewport,
}

impl OrchestrationDocument {
    pub fn validate(&self) -> ValidationReport {
        validate_orchestration(self)
    }
    pub fn compile(&self) -> Result<CompiledRouting, ValidationReport> {
        compile_orchestration(self)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FlowNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: FlowNodeKind,
    pub position: FlowPosition,
    #[serde(default)]
    pub data: FlowNodeData,
}

impl Serialize for FlowNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut node = serializer.serialize_struct("FlowNode", 4)?;
        node.serialize_field("id", &self.id)?;
        node.serialize_field("type", &self.kind)?;
        node.serialize_field("position", &self.position)?;
        node.serialize_field(
            "data",
            &FlowNodeDataByKind {
                kind: self.kind,
                data: &self.data,
            },
        )?;
        node.end()
    }
}

/// Kind-aware view so chain always emits `name`+`hops` (even when hops empty).
struct FlowNodeDataByKind<'a> {
    kind: FlowNodeKind,
    data: &'a FlowNodeData,
}

impl Serialize for FlowNodeDataByKind<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.kind {
            FlowNodeKind::Rule => {
                let mut data = serializer.serialize_struct("FlowNodeData", 2)?;
                if let Some(matcher) = &self.data.matcher {
                    data.serialize_field("matcher", matcher)?;
                }
                if let Some(priority) = self.data.priority {
                    data.serialize_field("priority", &priority)?;
                }
                data.end()
            }
            FlowNodeKind::Builtin => {
                let mut data = serializer.serialize_struct("FlowNodeData", 1)?;
                if let Some(builtin) = self.data.builtin {
                    data.serialize_field("builtin", &builtin)?;
                }
                data.end()
            }
            FlowNodeKind::Chain => {
                let mut data = serializer.serialize_struct("FlowNodeData", 2)?;
                data.serialize_field("name", &self.data.name)?;
                data.serialize_field("hops", &self.data.hops)?;
                data.end()
            }
            FlowNodeKind::Start | FlowNodeKind::End => {
                let data = serializer.serialize_struct("FlowNodeData", 0)?;
                data.end()
            }
            FlowNodeKind::NodeGroup => {
                let mut data = serializer.serialize_struct("FlowNodeData", 4)?;
                data.serialize_field("name", &self.data.name)?;
                data.serialize_field("policy", &self.data.policy)?;
                data.serialize_field("sources", &self.data.sources)?;
                if let Some(runtime_group_id) = &self.data.runtime_group_id {
                    data.serialize_field("runtime_group_id", runtime_group_id)?;
                }
                data.end()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeKind {
    Start,
    End,
    Rule,
    NodeGroup,
    Builtin,
    /// V3 legacy — stripped during migration. Not valid in V4 documents.
    Chain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for FlowViewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 0.85,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FlowNodeData {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_policy")]
    pub policy: String,
    #[serde(default)]
    pub sources: Vec<GroupSource>,
    /// Ordered hop list for `chain` nodes (reuses group source shapes).
    #[serde(default)]
    pub hops: Vec<GroupSource>,
    #[serde(default)]
    pub matcher: Option<RuleMatcher>,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(default)]
    pub builtin: Option<BuiltinKind>,
    #[serde(default)]
    pub runtime_group_id: Option<String>,
}

impl Default for FlowNodeData {
    fn default() -> Self {
        Self {
            name: String::new(),
            policy: default_policy(),
            sources: vec![],
            hops: vec![],
            matcher: None,
            priority: None,
            builtin: None,
            runtime_group_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleMatcher {
    pub kind: RuleMatcherKind,
    pub pattern: String,
    /// When true, the compiled expression is prefixed with `!` (invert match).
    #[serde(default)]
    pub invert: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RuleMatcherKind {
    DomainSuffix,
    DestinationCidr,
    /// domain(keyword: xxx)
    DomainKeyword,
    /// domain(full: xxx)
    DomainFull,
    /// domain(geosite:cn)
    Geosite,
    /// dip(geoip:cn)
    Geoip,
    /// sip(x.x.x.x/n)
    SourceCidr,
    /// sport(port) or sport(port1, port2)
    SourcePort,
    /// dport(port) or dport(port1, port2)
    DestPort,
    /// ipversion(4) or ipversion(6)
    IpVersion,
    /// pname(process_name)
    ProcessName,
    /// mac(xx:xx:xx:xx:xx:xx)
    MacAddress,
    /// l4proto(tcp) or l4proto(udp)
    Protocol,
}

/// Fixed evaluation band for routing rules.
///
/// dae evaluates top-to-bottom and stops at the first hit. Domain matches must
/// always win over geosite lists, which must win over IP/geoip matches,
/// regardless of the user-assigned `priority` number. Within a band, lower
/// `priority` is evaluated first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum RuleMatcherBand {
    Domain = 0,
    Geosite = 1,
    Ip = 2,
    Other = 3,
}

impl RuleMatcherKind {
    pub fn band(self) -> RuleMatcherBand {
        match self {
            Self::DomainSuffix | Self::DomainFull | Self::DomainKeyword => RuleMatcherBand::Domain,
            Self::Geosite => RuleMatcherBand::Geosite,
            Self::Geoip | Self::DestinationCidr | Self::SourceCidr | Self::IpVersion => {
                RuleMatcherBand::Ip
            }
            Self::SourcePort
            | Self::DestPort
            | Self::ProcessName
            | Self::MacAddress
            | Self::Protocol => RuleMatcherBand::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinKind {
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GroupSource {
    Node {
        id: String,
        #[serde(default = "default_weight")]
        weight: u32,
    },
    Subscription {
        id: String,
        #[serde(default = "default_weight")]
        weight: u32,
    },
    Group {
        id: String,
        #[serde(default = "default_weight")]
        weight: u32,
    },
}

impl GroupSource {
    pub fn id(&self) -> &str {
        match self {
            Self::Node { id, .. } | Self::Subscription { id, .. } | Self::Group { id, .. } => id,
        }
    }
    pub fn weight(&self) -> u32 {
        match self {
            Self::Node { weight, .. }
            | Self::Subscription { weight, .. }
            | Self::Group { weight, .. } => *weight,
        }
    }
    fn key(&self) -> (&'static str, &str) {
        match self {
            Self::Node { id, .. } => ("node", id),
            Self::Subscription { id, .. } => ("subscription", id),
            Self::Group { id, .. } => ("group", id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowEdge {
    pub id: String,
    pub source: String,
    pub target: String,
}

impl FlowEdge {
    pub fn new(id: &str, source: &str, target: &str) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            target: target.into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ValidationScope {
    Graph,
    Runtime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub scope: ValidationScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationReport {
    pub valid: bool,
    pub dae_compatible: bool,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledRoute {
    /// Structured form of the matcher, kept alongside the rendered dae
    /// expression so the dry-run tester can report which rule fired.
    pub matcher: RuleMatcher,
    pub priority: u32,
    /// dae-specific expression retained for the Linux renderer.
    pub condition: String,
    pub outbound: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledRouting {
    pub conditions: Vec<CompiledRoute>,
    pub fallback: String,
}

/// Traffic facts used to dry-run the compiled routing table.
///
/// Fields are optional; only matchers that can be evaluated from the provided
/// facts participate. Domain-only probes skip IP/geoip rules, and vice versa.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteProbe {
    /// Destination hostname (SNI / sniffed domain), lowercased by the matcher.
    #[serde(default)]
    pub domain: Option<String>,
    /// Destination IP (literal IPv4/IPv6).
    #[serde(default)]
    pub dest_ip: Option<String>,
    /// ISO-like geoip country/region codes (e.g. `us`, `cn`, `private`).
    #[serde(default)]
    pub geoip: Vec<String>,
    /// Geosite category codes (e.g. `netflix`, `google`).
    #[serde(default)]
    pub geosite: Vec<String>,
    /// L4 protocol when testing port/protocol rules: `tcp` or `udp`.
    #[serde(default)]
    pub protocol: Option<String>,
    /// Destination port.
    #[serde(default)]
    pub dest_port: Option<u16>,
}

/// One step of a dry-run walk: either a rule was considered, or the fallback won.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteTraceStep {
    /// `rule` | `fallback`
    pub kind: String,
    pub matched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub outbound: String,
    /// Why this step matched / was skipped / did not match.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteSimulation {
    /// Final outbound name (`direct` / group identifier).
    pub outbound: String,
    /// Whether any rule matched (false ⇒ fallback).
    pub matched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_priority: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_matcher_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_pattern: Option<String>,
    /// Ordered evaluation trace (band → priority order).
    pub steps: Vec<RouteTraceStep>,
}

impl CompiledRouting {
    /// Dry-run the compiled table against probe facts. First match wins.
    pub fn simulate(&self, probe: &RouteProbe) -> RouteSimulation {
        let mut steps = Vec::with_capacity(self.conditions.len() + 1);
        for (index, route) in self.conditions.iter().enumerate() {
            let decision = evaluate_route(route, probe);
            steps.push(RouteTraceStep {
                kind: "rule".into(),
                matched: decision.matched,
                rule_index: Some(index),
                priority: Some(route.priority),
                matcher_kind: Some(matcher_kind_name(route.matcher.kind).into()),
                pattern: Some(route.matcher.pattern.clone()),
                condition: Some(route.condition.clone()),
                outbound: route.outbound.clone(),
                reason: decision.reason,
            });
            if decision.matched {
                return RouteSimulation {
                    outbound: route.outbound.clone(),
                    matched: true,
                    matched_rule_index: Some(index),
                    matched_condition: Some(route.condition.clone()),
                    matched_priority: Some(route.priority),
                    matched_matcher_kind: Some(matcher_kind_name(route.matcher.kind).into()),
                    matched_pattern: Some(route.matcher.pattern.clone()),
                    steps,
                };
            }
        }
        steps.push(RouteTraceStep {
            kind: "fallback".into(),
            matched: true,
            rule_index: None,
            priority: None,
            matcher_kind: None,
            pattern: None,
            condition: None,
            outbound: self.fallback.clone(),
            reason: "no_rule_matched".into(),
        });
        RouteSimulation {
            outbound: self.fallback.clone(),
            matched: false,
            matched_rule_index: None,
            matched_condition: None,
            matched_priority: None,
            matched_matcher_kind: None,
            matched_pattern: None,
            steps,
        }
    }
}

struct MatchDecision {
    matched: bool,
    reason: String,
}

fn matcher_kind_name(kind: RuleMatcherKind) -> &'static str {
    match kind {
        RuleMatcherKind::DomainSuffix => "domain_suffix",
        RuleMatcherKind::DestinationCidr => "destination_cidr",
        RuleMatcherKind::DomainKeyword => "domain_keyword",
        RuleMatcherKind::DomainFull => "domain_full",
        RuleMatcherKind::Geosite => "geosite",
        RuleMatcherKind::Geoip => "geoip",
        RuleMatcherKind::SourceCidr => "source_cidr",
        RuleMatcherKind::SourcePort => "source_port",
        RuleMatcherKind::DestPort => "dest_port",
        RuleMatcherKind::IpVersion => "ip_version",
        RuleMatcherKind::ProcessName => "process_name",
        RuleMatcherKind::MacAddress => "mac_address",
        RuleMatcherKind::Protocol => "protocol",
    }
}

fn evaluate_route(route: &CompiledRoute, probe: &RouteProbe) -> MatchDecision {
    let raw = match_matcher(&route.matcher, probe);
    if route.matcher.invert {
        // Inverted rules only fire when the positive match would have succeeded
        // with the available facts. Missing facts stay skipped (not inverted).
        if raw.reason.starts_with("skip_") {
            return raw;
        }
        return MatchDecision {
            matched: !raw.matched,
            reason: if raw.matched {
                "inverted_positive_match".into()
            } else {
                "inverted_no_match".into()
            },
        };
    }
    raw
}

fn match_matcher(matcher: &RuleMatcher, probe: &RouteProbe) -> MatchDecision {
    match matcher.kind {
        RuleMatcherKind::DomainSuffix => {
            let Some(domain) = probe.domain.as_deref().map(normalize_domain) else {
                return skip("skip_no_domain");
            };
            let patterns = split_domain_list(&matcher.pattern);
            if patterns.is_empty() {
                return no("empty_pattern");
            }
            let hit = patterns
                .iter()
                .any(|pattern| domain == *pattern || domain.ends_with(&format!(".{pattern}")));
            yes_or_no(hit, "domain_suffix_match", "domain_suffix_miss")
        }
        RuleMatcherKind::DomainFull => {
            let Some(domain) = probe.domain.as_deref().map(normalize_domain) else {
                return skip("skip_no_domain");
            };
            let patterns = split_domain_list(&matcher.pattern);
            if patterns.is_empty() {
                return no("empty_pattern");
            }
            let hit = patterns.contains(&domain);
            yes_or_no(hit, "domain_full_match", "domain_full_miss")
        }
        RuleMatcherKind::DomainKeyword => {
            let Some(domain) = probe.domain.as_deref().map(|d| d.to_ascii_lowercase()) else {
                return skip("skip_no_domain");
            };
            let patterns = split_domain_list(&matcher.pattern);
            if patterns.is_empty() {
                return no("empty_pattern");
            }
            let hit = patterns.iter().any(|pattern| domain.contains(pattern));
            yes_or_no(hit, "domain_keyword_match", "domain_keyword_miss")
        }
        RuleMatcherKind::Geosite => {
            if probe.geosite.is_empty() {
                return skip("skip_no_geosite");
            }
            let codes = split_geo_codes(&matcher.pattern);
            let hit = probe
                .geosite
                .iter()
                .any(|code| codes.iter().any(|c| c.eq_ignore_ascii_case(code.trim())));
            yes_or_no(hit, "geosite_match", "geosite_miss")
        }
        RuleMatcherKind::Geoip => {
            if probe.geoip.is_empty() {
                return skip("skip_no_geoip");
            }
            let codes = split_geo_codes(&matcher.pattern);
            let hit = probe
                .geoip
                .iter()
                .any(|code| codes.iter().any(|c| c.eq_ignore_ascii_case(code.trim())));
            yes_or_no(hit, "geoip_match", "geoip_miss")
        }
        RuleMatcherKind::DestinationCidr => {
            let Some(ip) = probe.dest_ip.as_deref() else {
                return skip("skip_no_dest_ip");
            };
            match ip_in_cidr(ip, matcher.pattern.trim()) {
                Ok(true) => yes("cidr_match"),
                Ok(false) => no("cidr_miss"),
                Err(_) => no("invalid_cidr_or_ip"),
            }
        }
        RuleMatcherKind::SourceCidr => skip("skip_source_cidr_unsupported"),
        RuleMatcherKind::SourcePort => skip("skip_source_port_unsupported"),
        RuleMatcherKind::DestPort => {
            let Some(port) = probe.dest_port else {
                return skip("skip_no_dest_port");
            };
            let hit = port_list_contains(matcher.pattern.trim(), port);
            yes_or_no(hit, "dest_port_match", "dest_port_miss")
        }
        RuleMatcherKind::IpVersion => {
            let Some(ip) = probe.dest_ip.as_deref() else {
                return skip("skip_no_dest_ip");
            };
            let want = matcher.pattern.trim();
            let is_v4 = ip.parse::<std::net::Ipv4Addr>().is_ok();
            let is_v6 = ip.parse::<std::net::Ipv6Addr>().is_ok();
            let hit = (want == "4" && is_v4) || (want == "6" && is_v6);
            yes_or_no(hit, "ip_version_match", "ip_version_miss")
        }
        RuleMatcherKind::ProcessName => skip("skip_process_name_unsupported"),
        RuleMatcherKind::MacAddress => skip("skip_mac_unsupported"),
        RuleMatcherKind::Protocol => {
            let Some(proto) = probe.protocol.as_deref() else {
                return skip("skip_no_protocol");
            };
            let hit = proto.eq_ignore_ascii_case(matcher.pattern.trim());
            yes_or_no(hit, "protocol_match", "protocol_miss")
        }
    }
}

fn skip(reason: &str) -> MatchDecision {
    MatchDecision {
        matched: false,
        reason: reason.into(),
    }
}

fn yes(reason: &str) -> MatchDecision {
    MatchDecision {
        matched: true,
        reason: reason.into(),
    }
}

fn no(reason: &str) -> MatchDecision {
    MatchDecision {
        matched: false,
        reason: reason.into(),
    }
}

fn yes_or_no(hit: bool, yes_reason: &str, no_reason: &str) -> MatchDecision {
    if hit {
        yes(yes_reason)
    } else {
        no(no_reason)
    }
}

fn port_list_contains(pattern: &str, port: u16) -> bool {
    pattern
        .split([',', ' '])
        .filter(|s| !s.is_empty())
        .any(|part| {
            if let Some((a, b)) = part.split_once('-') {
                let Ok(lo) = a.trim().parse::<u16>() else {
                    return false;
                };
                let Ok(hi) = b.trim().parse::<u16>() else {
                    return false;
                };
                (lo.min(hi)..=lo.max(hi)).contains(&port)
            } else {
                part.trim().parse::<u16>().ok() == Some(port)
            }
        })
}

/// Return true when `ip` is contained in `cidr` (`x.x.x.x/n` or bare IP).
fn ip_in_cidr(ip: &str, cidr: &str) -> Result<bool, ()> {
    use std::net::IpAddr;
    let ip: IpAddr = ip.trim().parse().map_err(|_| ())?;
    let cidr = cidr.trim();
    if let Some((net, prefix)) = cidr.split_once('/') {
        let network: IpAddr = net.trim().parse().map_err(|_| ())?;
        let bits: u8 = prefix.trim().parse().map_err(|_| ())?;
        return Ok(ip_prefix_contains(network, bits, ip));
    }
    // bare IP acts as /32 or /128
    let network: IpAddr = cidr.parse().map_err(|_| ())?;
    Ok(ip == network)
}

fn ip_prefix_contains(network: std::net::IpAddr, prefix: u8, ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match (network, ip) {
        (IpAddr::V4(net), IpAddr::V4(addr)) => {
            if prefix > 32 {
                return false;
            }
            if prefix == 0 {
                return true;
            }
            let mask = u32::MAX << (32 - prefix);
            (u32::from(net) & mask) == (u32::from(addr) & mask)
        }
        (IpAddr::V6(net), IpAddr::V6(addr)) => {
            if prefix > 128 {
                return false;
            }
            if prefix == 0 {
                return true;
            }
            let net_bytes = net.octets();
            let addr_bytes = addr.octets();
            let full = (prefix / 8) as usize;
            let rem = prefix % 8;
            if net_bytes[..full] != addr_bytes[..full] {
                return false;
            }
            if rem == 0 {
                return true;
            }
            let mask = 0xffu8 << (8 - rem);
            (net_bytes[full] & mask) == (addr_bytes[full] & mask)
        }
        _ => false,
    }
}

impl Default for OrchestrationDocument {
    fn default() -> Self {
        Self {
            version: ORCHESTRATION_VERSION,
            nodes: vec![start_node(), default_group(), direct_builtin(), end_node()],
            edges: vec![FlowEdge::new("end-direct", "end", "direct")],
            viewport: FlowViewport::default(),
        }
    }
}

/// Ensure start/end/direct anchors and start→rule / end→terminal edges for v3 documents.
pub fn migrate_orchestration_document(
    mut document: OrchestrationDocument,
) -> OrchestrationDocument {
    // If already v3 and has start+end, still ensure invariants (idempotent).
    if !document.nodes.iter().any(|n| n.kind == FlowNodeKind::Start) {
        document.nodes.push(start_node());
    }
    if !document.nodes.iter().any(|n| n.kind == FlowNodeKind::End) {
        document.nodes.push(end_node());
    }
    if !document
        .nodes
        .iter()
        .any(|n| n.kind == FlowNodeKind::Builtin && n.data.builtin == Some(BuiltinKind::Direct))
    {
        document.nodes.push(direct_builtin());
    }
    // Ensure start → each rule
    let rule_ids: Vec<String> = document
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::Rule)
        .map(|n| n.id.clone())
        .collect();
    for rule_id in rule_ids {
        let has = document
            .edges
            .iter()
            .any(|e| e.source == "start" && e.target == rule_id);
        if !has {
            document.edges.push(FlowEdge::new(
                &format!("start-{rule_id}"),
                "start",
                &rule_id,
            ));
        }
    }
    // Ensure end has exactly one outbound to terminal; default direct if missing
    let end_outs: Vec<_> = document
        .edges
        .iter()
        .filter(|e| e.source == "end")
        .cloned()
        .collect();
    if end_outs.is_empty() {
        document
            .edges
            .push(FlowEdge::new("end-direct", "end", "direct"));
    }

    // V3 → V4: remove chain nodes and rewrite rule targets.
    if document.version == 3 {
        let chain_ids: HashSet<String> = document
            .nodes
            .iter()
            .filter(|n| n.kind == FlowNodeKind::Chain)
            .map(|n| n.id.clone())
            .collect();
        if !chain_ids.is_empty() {
            for edge in document.edges.iter_mut() {
                if !chain_ids.contains(&edge.target) {
                    continue;
                }
                let chain_node = document
                    .nodes
                    .iter()
                    .find(|n| n.id == edge.target)
                    .expect("chain id from document");
                let fallback = match chain_node.data.hops.as_slice() {
                    [GroupSource::Group { id, .. }] => {
                        if document
                            .nodes
                            .iter()
                            .any(|n| n.id == *id && n.kind == FlowNodeKind::NodeGroup)
                        {
                            id.clone()
                        } else {
                            "direct".to_string()
                        }
                    }
                    _ => "direct".to_string(),
                };
                edge.target = fallback;
            }
            document
                .edges
                .retain(|e| !chain_ids.contains(&e.source) && !chain_ids.contains(&e.target));
            document.nodes.retain(|n| n.kind != FlowNodeKind::Chain);
        }
    }

    document.version = ORCHESTRATION_VERSION;
    document
}

pub fn validate_orchestration(document: &OrchestrationDocument) -> ValidationReport {
    let mut issues = Vec::new();
    if document.version != ORCHESTRATION_VERSION {
        graph(&mut issues, "unsupported_version", None, None);
    }
    if !document.viewport.x.is_finite()
        || !document.viewport.y.is_finite()
        || !document.viewport.zoom.is_finite()
        || document.viewport.zoom <= 0.0
    {
        graph(&mut issues, "invalid_viewport", None, None);
    }
    let mut nodes = HashMap::new();
    for node in &document.nodes {
        if node.id.trim().is_empty() {
            graph(&mut issues, "node_id_required", Some(&node.id), None);
        }
        if nodes.insert(node.id.as_str(), node).is_some() {
            graph(&mut issues, "duplicate_node_id", Some(&node.id), None);
        }
        if !node.position.x.is_finite() || !node.position.y.is_finite() {
            graph(&mut issues, "invalid_node_position", Some(&node.id), None);
        }
    }

    let start_count = document
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::Start)
        .count();
    if start_count != 1 {
        graph(&mut issues, "start_required", None, None);
    }
    let end_count = document
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::End)
        .count();
    if end_count != 1 {
        graph(&mut issues, "end_required", None, None);
    }
    let directs: Vec<_> = document
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::Builtin && n.data.builtin == Some(BuiltinKind::Direct))
        .collect();
    if directs.len() != 1 {
        graph(&mut issues, "direct_required", None, None);
    }

    let mut outgoing: HashMap<&str, Vec<&FlowEdge>> = HashMap::new();
    let mut incoming: HashMap<&str, Vec<&FlowEdge>> = HashMap::new();
    let mut edge_ids = HashSet::new();
    let mut pairs = HashSet::new();
    for edge in &document.edges {
        if edge.id.trim().is_empty() {
            graph(&mut issues, "edge_id_required", None, Some(&edge.id));
        } else if !edge_ids.insert(edge.id.as_str()) {
            graph(&mut issues, "duplicate_edge_id", None, Some(&edge.id));
        }
        let (Some(source), Some(target)) = (
            nodes.get(edge.source.as_str()),
            nodes.get(edge.target.as_str()),
        ) else {
            graph(&mut issues, "dangling_edge", None, Some(&edge.id));
            continue;
        };
        if edge.source == edge.target {
            graph(
                &mut issues,
                "self_connection",
                Some(&edge.source),
                Some(&edge.id),
            );
        }
        if !pairs.insert((edge.source.as_str(), edge.target.as_str())) {
            graph(&mut issues, "duplicate_connection", None, Some(&edge.id));
        }
        if !edge_allowed(source, target) {
            graph(&mut issues, "invalid_connection", None, Some(&edge.id));
        }
        outgoing.entry(edge.source.as_str()).or_default().push(edge);
        incoming.entry(edge.target.as_str()).or_default().push(edge);
    }

    let mut names = HashSet::new();
    for node in &document.nodes {
        let outs = outgoing
            .get(node.id.as_str())
            .map_or(&[][..], Vec::as_slice);
        let ins = incoming
            .get(node.id.as_str())
            .map_or(&[][..], Vec::as_slice);
        match node.kind {
            FlowNodeKind::Rule => {
                validate_rule(node, outs, &mut issues);
                let from_start = ins
                    .iter()
                    .filter(|e| {
                        nodes
                            .get(e.source.as_str())
                            .is_some_and(|s| s.kind == FlowNodeKind::Start)
                    })
                    .count();
                if from_start != 1 {
                    graph(&mut issues, "rule_start_required", Some(&node.id), None);
                }
            }
            FlowNodeKind::NodeGroup => {
                if !outs.is_empty() {
                    graph(&mut issues, "group_terminal_required", Some(&node.id), None);
                }
                validate_group(node, &mut names, &mut issues);
            }
            FlowNodeKind::Builtin if node.data.builtin != Some(BuiltinKind::Direct) => {
                graph(&mut issues, "unsupported_builtin", Some(&node.id), None)
            }
            FlowNodeKind::Builtin => {
                if !outs.is_empty() {
                    graph(
                        &mut issues,
                        "builtin_terminal_required",
                        Some(&node.id),
                        None,
                    );
                }
            }
            FlowNodeKind::Start => {
                // no inbound required; outbounds already constrained by edge_allowed
            }
            FlowNodeKind::End => {
                if outs.len() != 1 {
                    graph(&mut issues, "end_target_required", Some(&node.id), None);
                } else if let Some(target) = nodes.get(outs[0].target.as_str()) {
                    if !matches!(target.kind, FlowNodeKind::NodeGroup | FlowNodeKind::Builtin) {
                        graph(&mut issues, "end_target_invalid", Some(&node.id), None);
                    }
                }
            }
            FlowNodeKind::Chain => {
                graph(&mut issues, "chain_unsupported", Some(&node.id), None);
            }
        }
    }
    validate_unique_rules(document, &mut issues);
    validate_group_source_cycles(document, &mut issues);
    deduplicate(&mut issues);
    let valid = !issues
        .iter()
        .any(|issue| issue.scope == ValidationScope::Graph);
    let dae_compatible = valid
        && !issues
            .iter()
            .any(|issue| issue.scope == ValidationScope::Runtime);
    ValidationReport {
        valid,
        dae_compatible,
        issues,
    }
}

fn edge_allowed(source: &FlowNode, target: &FlowNode) -> bool {
    match source.kind {
        FlowNodeKind::Start => target.kind == FlowNodeKind::Rule,
        FlowNodeKind::Rule => {
            matches!(target.kind, FlowNodeKind::NodeGroup | FlowNodeKind::Builtin)
        }
        FlowNodeKind::End => matches!(target.kind, FlowNodeKind::NodeGroup | FlowNodeKind::Builtin),
        _ => false,
    }
}

pub fn compile_orchestration(
    document: &OrchestrationDocument,
) -> Result<CompiledRouting, ValidationReport> {
    let report = validate_orchestration(document);
    if !report.valid || !report.dae_compatible {
        return Err(report);
    }
    let nodes: HashMap<_, _> = document
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut rules: Vec<_> = document
        .nodes
        .iter()
        .filter(|node| node.kind == FlowNodeKind::Rule)
        .collect();
    // domain → geosite → IP/geoip → other, then user priority, then stable id.
    rules.sort_by_key(|node| {
        let matcher = node.data.matcher.as_ref().expect("validated matcher");
        (
            matcher.kind.band(),
            node.data.priority.unwrap_or(u32::MAX),
            node.id.as_str(),
        )
    });
    let conditions = rules
        .into_iter()
        .map(|rule| {
            let edge = document
                .edges
                .iter()
                .find(|edge| edge.source == rule.id)
                .expect("validated rule target");
            let target = nodes[edge.target.as_str()];
            let matcher = rule.data.matcher.as_ref().expect("validated matcher");
            let outbound = outbound(target);
            CompiledRoute {
                matcher: normalized_matcher(matcher),
                priority: rule.data.priority.expect("validated priority"),
                condition: compile_matcher(matcher),
                outbound,
            }
        })
        .collect();

    let end = document
        .nodes
        .iter()
        .find(|n| n.kind == FlowNodeKind::End)
        .expect("validated end node");
    let end_edge = document
        .edges
        .iter()
        .find(|e| e.source == end.id)
        .expect("validated end target");
    let fallback_target = nodes[end_edge.target.as_str()];
    let fallback = outbound(fallback_target);

    Ok(CompiledRouting {
        conditions,
        fallback,
    })
}

fn validate_rule(node: &FlowNode, outputs: &[&FlowEdge], issues: &mut Vec<ValidationIssue>) {
    let Some(matcher) = &node.data.matcher else {
        graph(issues, "rule_matcher_required", Some(&node.id), None);
        return;
    };
    let pattern = matcher.pattern.trim();
    let valid = match matcher.kind {
        RuleMatcherKind::DomainSuffix | RuleMatcherKind::DomainFull => {
            let parts = split_domain_list(pattern);
            !parts.is_empty() && parts.iter().all(|part| valid_domain_suffix(part))
        }
        RuleMatcherKind::DestinationCidr => valid_cidr(pattern),
        RuleMatcherKind::DomainKeyword => {
            let parts = split_domain_list(pattern);
            !parts.is_empty()
                && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.len() <= 253)
        }
        RuleMatcherKind::Geosite => valid_geosite(pattern),
        RuleMatcherKind::Geoip => valid_geoip(pattern),
        RuleMatcherKind::SourceCidr => valid_cidr(pattern),
        RuleMatcherKind::SourcePort => valid_ports(pattern),
        RuleMatcherKind::DestPort => valid_ports(pattern),
        RuleMatcherKind::IpVersion => matches!(pattern, "4" | "6"),
        RuleMatcherKind::ProcessName => valid_process_name(pattern),
        RuleMatcherKind::MacAddress => valid_mac(pattern),
        RuleMatcherKind::Protocol => matches!(pattern.to_ascii_lowercase().as_str(), "tcp" | "udp"),
    };
    if !valid {
        graph(issues, "invalid_rule_pattern", Some(&node.id), None);
    }
    if outputs.len() != 1 {
        graph(issues, "rule_target_required", Some(&node.id), None);
    }
    if !matches!(node.data.priority, Some(1..=MAX_RULE_PRIORITY)) {
        graph(issues, "invalid_rule_priority", Some(&node.id), None);
    }
}

fn validate_group(node: &FlowNode, names: &mut HashSet<String>, issues: &mut Vec<ValidationIssue>) {
    let name = dae_identifier(&node.data.name);
    if normalized_dae_identifier(&node.data.name).is_none()
        || !names.insert(name.to_ascii_lowercase())
    {
        graph(issues, "invalid_group_name", Some(&node.id), None);
    }
    if is_reserved_dae_identifier(&name) {
        graph(issues, "reserved_group_name", Some(&node.id), None);
    }
    if !GROUP_POLICIES.contains(&node.data.policy.trim()) {
        graph(issues, "invalid_group_policy", Some(&node.id), None);
    }
    if node.data.sources.is_empty() {
        runtime(issues, "group_source_required", Some(&node.id), None);
    }
    let mut sources = HashSet::new();
    for source in &node.data.sources {
        if source.id().trim().is_empty() {
            graph(issues, "source_id_required", Some(&node.id), None);
        }
        if source.weight() == 0 || source.weight() > MAX_SOURCE_WEIGHT {
            graph(issues, "invalid_source_weight", Some(&node.id), None);
        }
        if !sources.insert(source.key()) {
            graph(issues, "duplicate_group_source", Some(&node.id), None);
        }
    }
}

fn validate_unique_rules(document: &OrchestrationDocument, issues: &mut Vec<ValidationIssue>) {
    let mut priorities = HashSet::new();
    let mut matchers = HashSet::new();
    for node in document
        .nodes
        .iter()
        .filter(|node| node.kind == FlowNodeKind::Rule)
    {
        if let Some(priority) = node.data.priority {
            if !priorities.insert(priority) {
                graph(issues, "duplicate_rule_priority", Some(&node.id), None);
            }
        }
        if let Some(matcher) = &node.data.matcher {
            let pattern = match matcher.kind {
                RuleMatcherKind::DomainSuffix | RuleMatcherKind::DomainFull => {
                    normalize_domain(&matcher.pattern)
                }
                RuleMatcherKind::MacAddress | RuleMatcherKind::Protocol => {
                    matcher.pattern.trim().to_ascii_lowercase()
                }
                _ => matcher.pattern.trim().to_ascii_lowercase(),
            };
            if !matchers.insert((matcher.kind, pattern)) {
                graph(issues, "duplicate_rule_matcher", Some(&node.id), None);
            }
        }
    }
}

fn validate_group_source_cycles(
    document: &OrchestrationDocument,
    issues: &mut Vec<ValidationIssue>,
) {
    let groups: HashMap<String, &FlowNode> = document
        .nodes
        .iter()
        .filter(|node| node.kind == FlowNodeKind::NodeGroup)
        .map(|node| (node.id.clone(), node))
        .collect();
    let mut references: HashMap<String, String> =
        groups.keys().map(|id| (id.clone(), id.clone())).collect();
    for group in groups.values() {
        if let Some(runtime_id) = group.data.runtime_group_id.as_deref() {
            references.insert(runtime_id.to_string(), group.id.clone());
        }
    }

    fn visit(
        group_id: &str,
        groups: &HashMap<String, &FlowNode>,
        references: &HashMap<String, String>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> bool {
        if visiting.contains(group_id) {
            return true;
        }
        if visited.contains(group_id) {
            return false;
        }
        visiting.insert(group_id.to_string());
        let cyclic = groups.get(group_id).is_some_and(|group| {
            group.data.sources.iter().any(|source| {
                let GroupSource::Group { id, .. } = source else {
                    return false;
                };
                references
                    .get(id)
                    .is_some_and(|target| visit(target, groups, references, visiting, visited))
            })
        });
        visiting.remove(group_id);
        visited.insert(group_id.to_string());
        cyclic
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for group in groups.values() {
        if visit(&group.id, &groups, &references, &mut visiting, &mut visited) {
            graph(issues, "group_source_cycle", Some(&group.id), None);
        }
    }
}

fn compile_matcher(matcher: &RuleMatcher) -> String {
    let pattern = matcher.pattern.trim();
    let expr = match matcher.kind {
        RuleMatcherKind::DomainSuffix => compile_domain_list("suffix", pattern),
        RuleMatcherKind::DestinationCidr => format!("dip({})", pattern),
        RuleMatcherKind::DomainKeyword => compile_domain_list("keyword", pattern),
        RuleMatcherKind::DomainFull => compile_domain_list("full", pattern),
        RuleMatcherKind::Geosite => compile_geo_list("domain", "geosite", pattern),
        RuleMatcherKind::Geoip => compile_geo_list("dip", "geoip", pattern),
        RuleMatcherKind::SourceCidr => format!("sip({})", pattern),
        RuleMatcherKind::SourcePort => format!("sport({})", pattern),
        RuleMatcherKind::DestPort => format!("dport({})", pattern),
        RuleMatcherKind::IpVersion => format!("ipversion({})", pattern),
        RuleMatcherKind::ProcessName => format!("pname({})", pattern),
        RuleMatcherKind::MacAddress => format!("mac({})", pattern.to_ascii_lowercase()),
        RuleMatcherKind::Protocol => format!("l4proto({})", pattern.to_ascii_lowercase()),
    };
    if matcher.invert {
        format!("!{}", expr)
    } else {
        expr
    }
}
fn normalized_matcher(matcher: &RuleMatcher) -> RuleMatcher {
    RuleMatcher {
        kind: matcher.kind,
        pattern: match matcher.kind {
            RuleMatcherKind::DomainSuffix
            | RuleMatcherKind::DomainFull
            | RuleMatcherKind::DomainKeyword => split_domain_list(&matcher.pattern).join(", "),
            RuleMatcherKind::Geosite | RuleMatcherKind::Geoip => {
                split_geo_codes(&matcher.pattern).join(", ")
            }
            RuleMatcherKind::MacAddress | RuleMatcherKind::Protocol => {
                matcher.pattern.trim().to_ascii_lowercase()
            }
            _ => matcher.pattern.trim().to_string(),
        },
        invert: matcher.invert,
    }
}
fn outbound(node: &FlowNode) -> String {
    match node.kind {
        FlowNodeKind::Builtin => "direct".into(),
        FlowNodeKind::NodeGroup => dae_identifier(&node.data.name),
        FlowNodeKind::Chain | FlowNodeKind::Rule | FlowNodeKind::Start | FlowNodeKind::End => {
            unreachable!("node kind cannot be a terminal outbound")
        }
    }
}

fn normalize_domain(value: &str) -> String {
    value.trim().trim_matches('.').to_ascii_lowercase()
}

/// Split multi-domain patterns: comma / whitespace / newline separated.
fn split_domain_list(value: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    value
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(normalize_domain)
        .filter(|part| !part.is_empty())
        .filter(|part| seen.insert(part.clone()))
        .collect()
}

/// `google.com, youtube.com` → `domain(suffix: google.com, suffix: youtube.com)`.
fn compile_domain_list(kind: &str, pattern: &str) -> String {
    let parts = split_domain_list(pattern);
    let joined = parts
        .into_iter()
        .map(|part| format!("{kind}: {part}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("domain({joined})")
}

fn valid_domain_suffix(value: &str) -> bool {
    let domain = normalize_domain(value);
    !domain.is_empty()
        && domain.len() <= 253
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
        })
}
fn valid_cidr(value: &str) -> bool {
    let Some((address, prefix)) = value.trim().split_once('/') else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    match address.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(_)) => prefix <= 32,
        Ok(std::net::IpAddr::V6(_)) => prefix <= 128,
        Err(_) => false,
    }
}

/// Validate geosite codes: single `cn` or multi `cn, category-ads`.
fn valid_geosite(value: &str) -> bool {
    let parts = split_geo_codes(value);
    !parts.is_empty()
        && parts.len() <= 32
        && parts.iter().all(|part| {
            part.len() <= 128
                && part.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'!' || b == b'@'
                })
        })
}

/// Validate geoip codes: single `cn` or multi `cn, private`.
fn valid_geoip(value: &str) -> bool {
    let parts = split_geo_codes(value);
    !parts.is_empty()
        && parts.len() <= 32
        && parts.iter().all(|part| {
            part.len() <= 64
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

fn split_geo_codes(value: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    value
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .filter(|part| seen.insert(part.clone()))
        .collect()
}

/// Compile multi-select geo patterns into dae list form:
/// `cn, private` → `dip(geoip:cn, geoip:private)`.
fn compile_geo_list(function: &str, prefix: &str, pattern: &str) -> String {
    let parts = split_geo_codes(pattern);
    let joined = parts
        .into_iter()
        .map(|part| format!("{prefix}:{part}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{function}({joined})")
}

/// Validate port list: single port `443`, range `8000-9000`, or comma-separated `80, 443`.
fn valid_ports(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() {
        return false;
    }
    v.split(',').all(|part| {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            let (Ok(s), Ok(e)) = (start.trim().parse::<u16>(), end.trim().parse::<u16>()) else {
                return false;
            };
            s <= e
        } else {
            part.parse::<u16>().is_ok()
        }
    })
}

/// Validate a process name (alphanumeric, dash, underscore, dot).
fn valid_process_name(value: &str) -> bool {
    let v = value.trim();
    !v.is_empty()
        && v.len() <= 256
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Validate a MAC address like `aa:bb:cc:dd:ee:ff`.
fn valid_mac(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    let parts: Vec<&str> = v.split(':').collect();
    if parts.len() != 6 {
        return false;
    }
    parts
        .iter()
        .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn start_node() -> FlowNode {
    FlowNode {
        id: "start".into(),
        kind: FlowNodeKind::Start,
        position: FlowPosition { x: 40.0, y: 200.0 },
        data: FlowNodeData::default(),
    }
}

fn end_node() -> FlowNode {
    FlowNode {
        id: "end".into(),
        kind: FlowNodeKind::End,
        position: FlowPosition { x: 760.0, y: 280.0 },
        data: FlowNodeData::default(),
    }
}

fn direct_builtin() -> FlowNode {
    FlowNode {
        id: "direct".into(),
        kind: FlowNodeKind::Builtin,
        position: FlowPosition { x: 520.0, y: 280.0 },
        data: FlowNodeData {
            builtin: Some(BuiltinKind::Direct),
            ..FlowNodeData::default()
        },
    }
}
fn default_group() -> FlowNode {
    FlowNode {
        id: "group-default".into(),
        kind: FlowNodeKind::NodeGroup,
        position: FlowPosition { x: 500.0, y: 100.0 },
        data: FlowNodeData {
            name: "proxy_01".into(),
            ..FlowNodeData::default()
        },
    }
}
fn default_version() -> u32 {
    ORCHESTRATION_VERSION
}
fn default_policy() -> String {
    "min_moving_avg".into()
}
fn default_weight() -> u32 {
    1
}
fn deduplicate(issues: &mut Vec<ValidationIssue>) {
    let mut seen = HashSet::new();
    issues.retain(|i| {
        seen.insert((
            i.scope.clone(),
            i.code.clone(),
            i.node_id.clone(),
            i.edge_id.clone(),
        ))
    });
}
fn graph(
    issues: &mut Vec<ValidationIssue>,
    code: &str,
    node_id: Option<&str>,
    edge_id: Option<&str>,
) {
    issue(issues, ValidationScope::Graph, code, node_id, edge_id)
}
fn runtime(
    issues: &mut Vec<ValidationIssue>,
    code: &str,
    node_id: Option<&str>,
    edge_id: Option<&str>,
) {
    issue(issues, ValidationScope::Runtime, code, node_id, edge_id)
}
fn issue(
    issues: &mut Vec<ValidationIssue>,
    scope: ValidationScope,
    code: &str,
    node_id: Option<&str>,
    edge_id: Option<&str>,
) {
    issues.push(ValidationIssue {
        code: code.into(),
        scope,
        node_id: node_id.map(str::to_string),
        edge_id: edge_id.map(str::to_string),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, kind: RuleMatcherKind, pattern: &str, priority: Option<u32>) -> FlowNode {
        FlowNode {
            id: id.into(),
            kind: FlowNodeKind::Rule,
            position: FlowPosition { x: 0.0, y: 0.0 },
            data: FlowNodeData {
                matcher: Some(RuleMatcher {
                    kind,
                    pattern: pattern.into(),
                    invert: false,
                }),
                priority,
                ..FlowNodeData::default()
            },
        }
    }
    fn group() -> FlowNode {
        FlowNode {
            id: "group".into(),
            kind: FlowNodeKind::NodeGroup,
            position: FlowPosition { x: 400.0, y: 0.0 },
            data: FlowNodeData {
                name: "Proxy".into(),
                sources: vec![GroupSource::Node {
                    id: "node-1".into(),
                    weight: 1,
                }],
                ..FlowNodeData::default()
            },
        }
    }

    fn base_terminals() -> Vec<FlowNode> {
        vec![start_node(), end_node(), direct_builtin(), group()]
    }

    fn chain_node(id: &str, name: &str, hops: Vec<GroupSource>) -> FlowNode {
        FlowNode {
            id: id.into(),
            kind: FlowNodeKind::Chain,
            position: FlowPosition { x: 500.0, y: 0.0 },
            data: FlowNodeData {
                name: name.into(),
                hops,
                ..FlowNodeData::default()
            },
        }
    }

    #[test]
    fn migrates_v3_document_with_chain_nodes_to_group_target() {
        let mut nodes = base_terminals();
        nodes.push(chain_node(
            "chain-1",
            "via",
            vec![GroupSource::Group {
                id: "group".into(),
                weight: 1,
            }],
        ));
        nodes.push(rule(
            "rule-a",
            RuleMatcherKind::DomainSuffix,
            "example.com",
            Some(1),
        ));
        let document = OrchestrationDocument {
            version: 3,
            nodes,
            edges: vec![
                FlowEdge::new("s-r", "start", "rule-a"),
                FlowEdge::new("r-c", "rule-a", "chain-1"),
                FlowEdge::new("e-d", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let migrated = migrate_orchestration_document(document);
        assert_eq!(migrated.version, ORCHESTRATION_VERSION);
        assert!(!migrated.nodes.iter().any(|n| n.kind == FlowNodeKind::Chain));
        assert!(migrated
            .edges
            .iter()
            .any(|e| e.source == "rule-a" && e.target == "group"));
        assert!(migrated.compile().is_ok());
    }

    #[test]
    fn migrates_v3_chain_with_node_hop_to_direct() {
        let mut nodes = base_terminals();
        nodes.push(chain_node(
            "chain-1",
            "via",
            vec![GroupSource::Node {
                id: "node-1".into(),
                weight: 1,
            }],
        ));
        nodes.push(rule(
            "rule-a",
            RuleMatcherKind::DomainSuffix,
            "example.com",
            Some(1),
        ));
        let document = OrchestrationDocument {
            version: 3,
            nodes,
            edges: vec![
                FlowEdge::new("s-r", "start", "rule-a"),
                FlowEdge::new("r-c", "rule-a", "chain-1"),
                FlowEdge::new("e-d", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let migrated = migrate_orchestration_document(document);
        assert!(!migrated.nodes.iter().any(|n| n.kind == FlowNodeKind::Chain));
        assert!(migrated
            .edges
            .iter()
            .any(|e| e.source == "rule-a" && e.target == "direct"));
    }

    #[test]
    fn v4_rejects_chain_nodes_in_validation() {
        let mut nodes = base_terminals();
        nodes.push(chain_node("chain-1", "via", vec![]));
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes,
            edges: vec![FlowEdge::new("e-d", "end", "direct")],
            viewport: FlowViewport::default(),
        };
        let report = document.validate();
        assert!(!report.valid);
        assert!(report.issues.iter().any(|i| i.code == "chain_unsupported"));
    }

    #[test]
    fn empty_chain_still_serializes_name_and_hops_for_legacy() {
        let node = chain_node("chain-1", "via", vec![]);
        let value = serde_json::to_value(&node).unwrap();
        assert_eq!(value["type"], "chain");
        assert_eq!(value["data"]["name"], "via");
        assert_eq!(value["data"]["hops"], serde_json::json!([]));
    }

    #[test]
    fn compile_uses_end_fallback_group() {
        let mut nodes = base_terminals();
        nodes.push(rule(
            "rule-a",
            RuleMatcherKind::DomainSuffix,
            "example.com",
            Some(1),
        ));
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes,
            edges: vec![
                FlowEdge::new("s-r", "start", "rule-a"),
                FlowEdge::new("r-d", "rule-a", "direct"),
                FlowEdge::new("e-g", "end", "group"),
            ],
            viewport: FlowViewport::default(),
        };
        let compiled = document.compile().expect("compile");
        assert_eq!(compiled.fallback, dae_identifier("Proxy"));
    }

    #[test]
    fn rejects_rule_without_start_edge_and_illegal_edges() {
        let mut nodes = base_terminals();
        nodes.push(rule(
            "rule-a",
            RuleMatcherKind::DomainSuffix,
            "example.com",
            Some(1),
        ));
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes,
            edges: vec![
                FlowEdge::new("r-d", "rule-a", "direct"),
                FlowEdge::new("e-d", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let report = document.validate();
        assert!(!report.valid);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == "rule_start_required"));
    }

    #[test]
    fn default_document_has_start_end_and_fallback_edge() {
        let document = OrchestrationDocument::default();
        assert_eq!(document.version, ORCHESTRATION_VERSION);
        assert!(document
            .nodes
            .iter()
            .any(|n| n.kind == FlowNodeKind::Start && n.id == "start"));
        assert!(document
            .nodes
            .iter()
            .any(|n| n.kind == FlowNodeKind::End && n.id == "end"));
        assert!(document
            .nodes
            .iter()
            .any(|n| n.kind == FlowNodeKind::Builtin && n.id == "direct"));
        assert!(document
            .edges
            .iter()
            .any(|e| e.source == "end" && e.target == "direct"));
    }

    #[test]
    fn migrates_v2_document_injects_start_end_and_start_rule_edges() {
        let v2 = OrchestrationDocument {
            version: 2,
            nodes: vec![
                rule(
                    "rule-domain",
                    RuleMatcherKind::DomainSuffix,
                    "example.com",
                    Some(1),
                ),
                group(),
                direct_builtin(),
            ],
            edges: vec![FlowEdge::new("r-g", "rule-domain", "group")],
            viewport: FlowViewport::default(),
        };
        let migrated = migrate_orchestration_document(v2);
        assert_eq!(migrated.version, ORCHESTRATION_VERSION);
        assert!(migrated.nodes.iter().any(|n| n.id == "start"));
        assert!(migrated.nodes.iter().any(|n| n.id == "end"));
        assert!(migrated
            .edges
            .iter()
            .any(|e| e.source == "start" && e.target == "rule-domain"));
        assert!(migrated
            .edges
            .iter()
            .any(|e| e.source == "end" && e.target == "direct"));
        // original rule target preserved
        assert!(migrated
            .edges
            .iter()
            .any(|e| e.source == "rule-domain" && e.target == "group"));
    }

    #[test]
    fn default_document_contains_one_group_and_fixed_direct() {
        let document = OrchestrationDocument::default();
        assert_eq!(document.version, ORCHESTRATION_VERSION);
        assert!(document
            .nodes
            .iter()
            .any(|n| n.kind == FlowNodeKind::NodeGroup && n.id == "group-default"));
        assert!(document
            .nodes
            .iter()
            .any(|n| n.kind == FlowNodeKind::Builtin && n.id == "direct"));
        assert!(document
            .edges
            .iter()
            .any(|e| e.source == "end" && e.target == "direct"));
        assert!(document.validate().valid);
        assert!(!document.validate().dae_compatible);
        assert!(document
            .validate()
            .issues
            .iter()
            .any(|issue| issue.code == "group_source_required"));
    }

    #[test]
    fn multi_geo_codes_compile_to_list() {
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                rule(
                    "rule-geoip",
                    RuleMatcherKind::Geoip,
                    "cn, private, us",
                    Some(1),
                ),
                rule(
                    "rule-geosite",
                    RuleMatcherKind::Geosite,
                    "cn category-ads",
                    Some(2),
                ),
                group(),
                direct_builtin(),
                end_node(),
            ],
            edges: vec![
                FlowEdge::new("start-geoip", "start", "rule-geoip"),
                FlowEdge::new("start-geosite", "start", "rule-geosite"),
                FlowEdge::new("geoip-direct", "rule-geoip", "direct"),
                FlowEdge::new("geosite-group", "rule-geosite", "group"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let compiled = document.compile().expect("compile multi geo");
        // geosite band always precedes IP/geoip, even when geoip has lower priority.
        assert_eq!(
            compiled.conditions[0].condition,
            "domain(geosite:cn, geosite:category-ads)"
        );
        assert_eq!(
            compiled.conditions[1].condition,
            "dip(geoip:cn, geoip:private, geoip:us)"
        );
    }

    #[test]
    fn rules_compile_band_first_then_priority_and_fall_back_to_direct() {
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                rule(
                    "rule-domain",
                    RuleMatcherKind::DomainSuffix,
                    " .Example.COM. ",
                    Some(2),
                ),
                rule(
                    "rule-cidr",
                    RuleMatcherKind::DestinationCidr,
                    "10.0.0.0/8",
                    Some(1),
                ),
                group(),
                direct_builtin(),
                end_node(),
            ],
            edges: vec![
                FlowEdge::new("start-domain", "start", "rule-domain"),
                FlowEdge::new("start-cidr", "start", "rule-cidr"),
                FlowEdge::new("domain-group", "rule-domain", "group"),
                FlowEdge::new("cidr-direct", "rule-cidr", "direct"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        assert_eq!(
            document.compile().unwrap(),
            CompiledRouting {
                conditions: vec![
                    // Domain band wins over IP band even with a higher priority number.
                    CompiledRoute {
                        matcher: RuleMatcher {
                            kind: RuleMatcherKind::DomainSuffix,
                            pattern: "example.com".into(),
                            invert: false,
                        },
                        priority: 2,
                        condition: "domain(suffix: example.com)".into(),
                        outbound: "Proxy".into()
                    },
                    CompiledRoute {
                        matcher: RuleMatcher {
                            kind: RuleMatcherKind::DestinationCidr,
                            pattern: "10.0.0.0/8".into(),
                            invert: false,
                        },
                        priority: 1,
                        condition: "dip(10.0.0.0/8)".into(),
                        outbound: "direct".into()
                    }
                ],
                fallback: "direct".into()
            }
        );
    }

    #[test]
    fn rules_compile_domain_before_geosite_before_geoip() {
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                rule("rule-geoip", RuleMatcherKind::Geoip, "us", Some(1)),
                rule(
                    "rule-domain",
                    RuleMatcherKind::DomainSuffix,
                    "fast.com",
                    Some(99),
                ),
                rule(
                    "rule-geosite",
                    RuleMatcherKind::Geosite,
                    "netflix",
                    Some(50),
                ),
                group(),
                direct_builtin(),
                end_node(),
            ],
            edges: vec![
                FlowEdge::new("start-geoip", "start", "rule-geoip"),
                FlowEdge::new("start-domain", "start", "rule-domain"),
                FlowEdge::new("start-geosite", "start", "rule-geosite"),
                FlowEdge::new("geoip-group", "rule-geoip", "group"),
                FlowEdge::new("domain-group", "rule-domain", "group"),
                FlowEdge::new("geosite-group", "rule-geosite", "group"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let compiled = document.compile().expect("compile band order");
        assert_eq!(
            compiled
                .conditions
                .iter()
                .map(|route| route.condition.as_str())
                .collect::<Vec<_>>(),
            vec![
                "domain(suffix: fast.com)",
                "domain(geosite:netflix)",
                "dip(geoip:us)",
            ]
        );
    }

    #[test]
    fn multi_domain_suffix_compiles_and_matches_any() {
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                rule(
                    "rule-multi",
                    RuleMatcherKind::DomainSuffix,
                    "google.com, youtube.com\nfast.com",
                    Some(1),
                ),
                group(),
                direct_builtin(),
                end_node(),
            ],
            edges: vec![
                FlowEdge::new("s-multi", "start", "rule-multi"),
                FlowEdge::new("multi-group", "rule-multi", "group"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let compiled = document.compile().expect("compile multi domain");
        assert_eq!(
            compiled.conditions[0].condition,
            "domain(suffix: google.com, suffix: youtube.com, suffix: fast.com)"
        );
        assert_eq!(
            compiled.conditions[0].matcher.pattern,
            "google.com, youtube.com, fast.com"
        );
        let hit = compiled.simulate(&RouteProbe {
            domain: Some("www.youtube.com".into()),
            ..Default::default()
        });
        assert!(hit.matched);
        assert_eq!(hit.outbound, "Proxy");
        let miss = compiled.simulate(&RouteProbe {
            domain: Some("example.org".into()),
            ..Default::default()
        });
        assert!(!miss.matched);
    }

    #[test]
    fn rejects_invalid_rule_targets_and_non_terminal_groups() {
        let mut document = OrchestrationDocument::default();
        document.nodes.extend([
            rule(
                "rule",
                RuleMatcherKind::DomainSuffix,
                "example.com",
                Some(1),
            ),
            group(),
        ]);
        document.edges = vec![
            FlowEdge::new("start-rule", "start", "rule"),
            FlowEdge::new("rule-group", "rule", "group"),
            FlowEdge::new("group-direct", "group", "direct"),
            FlowEdge::new("end-direct", "end", "direct"),
        ];
        let report = document.validate();
        assert!(!report.valid);
        assert!(report.issues.iter().any(|i| i.code == "invalid_connection"));
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == "group_terminal_required"));
    }

    #[test]
    fn rejects_duplicate_rule_order_and_reserved_group_names() {
        let mut first = rule("r1", RuleMatcherKind::DomainSuffix, "example.com", Some(1));
        let second = rule(
            "r2",
            RuleMatcherKind::DomainSuffix,
            " example.com ",
            Some(1),
        );
        first.position.x = 0.0;
        let mut blocked = group();
        blocked.data.name = "block".into();
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                end_node(),
                first,
                second,
                blocked,
                direct_builtin(),
            ],
            edges: vec![
                FlowEdge::new("s-r1", "start", "r1"),
                FlowEdge::new("s-r2", "start", "r2"),
                FlowEdge::new("r1-direct", "r1", "direct"),
                FlowEdge::new("r2-direct", "r2", "direct"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let report = document.validate();
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "duplicate_rule_priority"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "duplicate_rule_matcher"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "reserved_group_name"));
    }

    #[test]
    fn serializes_only_the_data_fields_for_each_frontend_node_kind() {
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                rule("rule", RuleMatcherKind::DomainSuffix, "example.com", None),
                group(),
                direct_builtin(),
                start_node(),
                end_node(),
                FlowNode {
                    id: "chain-empty".into(),
                    kind: FlowNodeKind::Chain,
                    position: FlowPosition { x: 0.0, y: 0.0 },
                    data: FlowNodeData {
                        name: "via".into(),
                        ..FlowNodeData::default()
                    },
                },
            ],
            edges: vec![],
            viewport: FlowViewport::default(),
        };
        let value = serde_json::to_value(document).unwrap();
        assert_eq!(value["nodes"][0]["type"], "rule");
        assert!(value["nodes"][0]["data"].get("matcher").is_some());
        assert!(value["nodes"][0]["data"].get("name").is_none());
        assert_eq!(value["nodes"][1]["data"]["name"], "Proxy");
        assert!(value["nodes"][1]["data"].get("matcher").is_none());
        assert_eq!(value["nodes"][2]["data"]["builtin"], "direct");
        assert!(value["nodes"][2]["data"].get("policy").is_none());
        assert_eq!(value["nodes"][3]["data"], serde_json::json!({}));
        assert_eq!(value["nodes"][4]["data"], serde_json::json!({}));
        assert_eq!(value["nodes"][5]["data"]["name"], "via");
        assert_eq!(value["nodes"][5]["data"]["hops"], serde_json::json!([]));
        assert!(value["nodes"][5]["data"].get("policy").is_none());
    }

    #[test]
    fn ignores_legacy_edge_data() {
        let edge: FlowEdge = serde_json::from_str(r#"{"id":"e","source":"r","target":"direct","data":{"condition":"domain(old)","is_fallback":true}}"#).unwrap();
        assert!(serde_json::to_value(edge).unwrap().get("data").is_none());
    }

    #[test]
    fn rejects_missing_priority_invalid_domains_and_unknown_policies() {
        let mut invalid_group = group();
        invalid_group.data.policy = "custom(policy)".into();
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                end_node(),
                rule("rule", RuleMatcherKind::DomainSuffix, "bad..example", None),
                invalid_group,
                direct_builtin(),
            ],
            edges: vec![
                FlowEdge::new("s-r", "start", "rule"),
                FlowEdge::new("rule-group", "rule", "group"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        let report = document.validate();
        for code in [
            "invalid_rule_pattern",
            "invalid_rule_priority",
            "invalid_group_policy",
        ] {
            assert!(report.issues.iter().any(|issue| issue.code == code));
        }
    }

    #[test]
    fn compiles_group_targets_with_the_renderer_identifier() {
        let mut target = group();
        target.data.name = "Hong Kong / Auto".into();
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                end_node(),
                rule(
                    "rule",
                    RuleMatcherKind::DomainSuffix,
                    "example.com",
                    Some(1),
                ),
                target,
                direct_builtin(),
            ],
            edges: vec![
                FlowEdge::new("s-r", "start", "rule"),
                FlowEdge::new("rule-group", "rule", "group"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        assert_eq!(
            document.compile().unwrap().conditions[0].outbound,
            "Hong_Kong_Auto"
        );
    }

    #[test]
    fn rejects_cycles_between_composed_groups() {
        let mut first = group();
        first.id = "first".into();
        first.data.name = "first".into();
        first.data.sources = vec![GroupSource::Group {
            id: "second".into(),
            weight: 1,
        }];
        let mut second = group();
        second.id = "second".into();
        second.data.name = "second".into();
        second.data.sources = vec![GroupSource::Group {
            id: "first".into(),
            weight: 1,
        }];
        let document = OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![start_node(), end_node(), first, second, direct_builtin()],
            edges: vec![FlowEdge::new("end-direct", "end", "direct")],
            viewport: FlowViewport::default(),
        };
        assert!(document
            .validate()
            .issues
            .iter()
            .any(|issue| issue.code == "group_source_cycle"));
    }

    fn sample_routing_document() -> OrchestrationDocument {
        OrchestrationDocument {
            version: ORCHESTRATION_VERSION,
            nodes: vec![
                start_node(),
                rule(
                    "rule-domain",
                    RuleMatcherKind::DomainSuffix,
                    "fast.com",
                    Some(1),
                ),
                rule("rule-geosite", RuleMatcherKind::Geosite, "netflix", Some(2)),
                rule("rule-geoip", RuleMatcherKind::Geoip, "us", Some(3)),
                rule(
                    "rule-cidr",
                    RuleMatcherKind::DestinationCidr,
                    "10.0.0.0/8",
                    Some(4),
                ),
                group(),
                direct_builtin(),
                end_node(),
            ],
            edges: vec![
                FlowEdge::new("s-domain", "start", "rule-domain"),
                FlowEdge::new("s-geosite", "start", "rule-geosite"),
                FlowEdge::new("s-geoip", "start", "rule-geoip"),
                FlowEdge::new("s-cidr", "start", "rule-cidr"),
                FlowEdge::new("domain-group", "rule-domain", "group"),
                FlowEdge::new("geosite-group", "rule-geosite", "group"),
                FlowEdge::new("geoip-group", "rule-geoip", "group"),
                FlowEdge::new("cidr-direct", "rule-cidr", "direct"),
                FlowEdge::new("end-direct", "end", "direct"),
            ],
            viewport: FlowViewport::default(),
        }
    }

    #[test]
    fn simulate_domain_hits_before_geoip() {
        let compiled = sample_routing_document().compile().unwrap();
        let result = compiled.simulate(&RouteProbe {
            domain: Some("api.fast.com".into()),
            dest_ip: Some("1.1.1.1".into()),
            geoip: vec!["us".into()],
            ..Default::default()
        });
        assert!(result.matched);
        assert_eq!(result.outbound, "Proxy");
        assert_eq!(
            result.matched_matcher_kind.as_deref(),
            Some("domain_suffix")
        );
        assert_eq!(result.matched_pattern.as_deref(), Some("fast.com"));
    }

    #[test]
    fn simulate_geoip_when_no_domain_rule_hits() {
        let compiled = sample_routing_document().compile().unwrap();
        let result = compiled.simulate(&RouteProbe {
            dest_ip: Some("8.8.8.8".into()),
            geoip: vec!["us".into()],
            ..Default::default()
        });
        assert!(result.matched);
        assert_eq!(result.outbound, "Proxy");
        assert_eq!(result.matched_matcher_kind.as_deref(), Some("geoip"));
        // Domain/geosite rules without facts are skipped, not matched.
        assert!(result.steps.iter().any(|s| s.reason == "skip_no_domain"));
        assert!(result.steps.iter().any(|s| s.reason == "skip_no_geosite"));
    }

    #[test]
    fn simulate_geosite_option_hits_group() {
        let compiled = sample_routing_document().compile().unwrap();
        let result = compiled.simulate(&RouteProbe {
            geosite: vec!["netflix".into()],
            ..Default::default()
        });
        assert!(result.matched);
        assert_eq!(result.matched_matcher_kind.as_deref(), Some("geosite"));
        assert_eq!(result.outbound, "Proxy");
    }

    #[test]
    fn simulate_cidr_and_fallback() {
        let compiled = sample_routing_document().compile().unwrap();
        let private = compiled.simulate(&RouteProbe {
            dest_ip: Some("10.1.2.3".into()),
            ..Default::default()
        });
        assert!(private.matched);
        assert_eq!(private.outbound, "direct");
        assert_eq!(
            private.matched_matcher_kind.as_deref(),
            Some("destination_cidr")
        );

        let miss = compiled.simulate(&RouteProbe {
            dest_ip: Some("1.2.3.4".into()),
            geoip: vec!["jp".into()],
            ..Default::default()
        });
        assert!(!miss.matched);
        assert_eq!(miss.outbound, "direct");
        assert_eq!(miss.steps.last().map(|s| s.kind.as_str()), Some("fallback"));
    }

    #[test]
    fn ip_prefix_helpers_cover_v4_and_bare() {
        assert_eq!(ip_in_cidr("10.0.0.5", "10.0.0.0/8"), Ok(true));
        assert_eq!(ip_in_cidr("11.0.0.5", "10.0.0.0/8"), Ok(false));
        assert_eq!(ip_in_cidr("1.2.3.4", "1.2.3.4"), Ok(true));
        assert!(ip_in_cidr("not-an-ip", "10.0.0.0/8").is_err());
    }
}
