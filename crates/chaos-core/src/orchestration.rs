//! Persisted V2 orchestration graph, validation, and dae routing compilation.

use std::collections::{HashMap, HashSet};

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use crate::config_render::{dae_identifier, is_reserved_dae_identifier, normalized_dae_identifier};

pub const ORCHESTRATION_VERSION: u32 = 2;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: FlowNodeKind,
    pub position: FlowPosition,
    #[serde(default)]
    pub data: FlowNodeData,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeKind {
    Rule,
    NodeGroup,
    Builtin,
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
    #[serde(default)]
    pub matcher: Option<RuleMatcher>,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(default)]
    pub builtin: Option<BuiltinKind>,
    #[serde(default)]
    pub runtime_group_id: Option<String>,
}

impl Serialize for FlowNodeData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut data = serializer.serialize_struct("FlowNodeData", 5)?;
        if let Some(matcher) = &self.matcher {
            data.serialize_field("matcher", matcher)?;
            if let Some(priority) = self.priority {
                data.serialize_field("priority", &priority)?;
            }
        } else if let Some(builtin) = self.builtin {
            data.serialize_field("builtin", &builtin)?;
        } else {
            data.serialize_field("name", &self.name)?;
            data.serialize_field("policy", &self.policy)?;
            data.serialize_field("sources", &self.sources)?;
            if let Some(runtime_group_id) = &self.runtime_group_id {
                data.serialize_field("runtime_group_id", runtime_group_id)?;
            }
        }
        data.end()
    }
}

impl Default for FlowNodeData {
    fn default() -> Self {
        Self {
            name: String::new(),
            policy: default_policy(),
            sources: vec![],
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RuleMatcherKind {
    DomainSuffix,
    DestinationCidr,
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
    /// Backend-neutral matcher retained for Windows and future renderers.
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

impl Default for OrchestrationDocument {
    fn default() -> Self {
        Self {
            version: ORCHESTRATION_VERSION,
            nodes: vec![default_group(), direct_builtin()],
            edges: vec![],
            viewport: FlowViewport::default(),
        }
    }
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
    let directs: Vec<_> = document
        .nodes
        .iter()
        .filter(|n| n.kind == FlowNodeKind::Builtin && n.data.builtin == Some(BuiltinKind::Direct))
        .collect();
    if directs.len() != 1 {
        graph(&mut issues, "direct_required", None, None);
    }

    let mut outgoing: HashMap<&str, Vec<&FlowEdge>> = HashMap::new();
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
        if source.kind != FlowNodeKind::Rule
            || !matches!(target.kind, FlowNodeKind::NodeGroup | FlowNodeKind::Builtin)
        {
            graph(&mut issues, "invalid_connection", None, Some(&edge.id));
        }
        outgoing.entry(edge.source.as_str()).or_default().push(edge);
    }
    let mut names = HashSet::new();
    for node in &document.nodes {
        match node.kind {
            FlowNodeKind::Rule => validate_rule(
                node,
                outgoing
                    .get(node.id.as_str())
                    .map_or(&[][..], Vec::as_slice),
                &mut issues,
            ),
            FlowNodeKind::NodeGroup => {
                if outgoing.contains_key(node.id.as_str()) {
                    graph(&mut issues, "group_terminal_required", Some(&node.id), None);
                }
                validate_group(node, &mut names, &mut issues);
            }
            FlowNodeKind::Builtin if node.data.builtin != Some(BuiltinKind::Direct) => {
                graph(&mut issues, "unsupported_builtin", Some(&node.id), None)
            }
            FlowNodeKind::Builtin => {
                if outgoing.contains_key(node.id.as_str()) {
                    graph(
                        &mut issues,
                        "builtin_terminal_required",
                        Some(&node.id),
                        None,
                    );
                }
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
    rules.sort_by_key(|node| (node.data.priority.unwrap_or(u32::MAX), node.id.as_str()));
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
            CompiledRoute {
                matcher: normalized_matcher(matcher),
                priority: rule.data.priority.expect("validated priority"),
                condition: compile_matcher(matcher),
                outbound: outbound(target),
            }
        })
        .collect();
    Ok(CompiledRouting {
        conditions,
        fallback: "direct".into(),
    })
}

fn validate_rule(node: &FlowNode, outputs: &[&FlowEdge], issues: &mut Vec<ValidationIssue>) {
    let Some(matcher) = &node.data.matcher else {
        graph(issues, "rule_matcher_required", Some(&node.id), None);
        return;
    };
    let pattern = matcher.pattern.trim();
    let valid = match matcher.kind {
        RuleMatcherKind::DomainSuffix => valid_domain_suffix(pattern),
        RuleMatcherKind::DestinationCidr => valid_cidr(pattern),
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
                RuleMatcherKind::DomainSuffix => normalize_domain(&matcher.pattern),
                RuleMatcherKind::DestinationCidr => matcher.pattern.trim().to_ascii_lowercase(),
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
    match matcher.kind {
        RuleMatcherKind::DomainSuffix => {
            format!("domain(suffix: {})", normalize_domain(&matcher.pattern))
        }
        RuleMatcherKind::DestinationCidr => format!("dip({})", matcher.pattern.trim()),
    }
}
fn normalized_matcher(matcher: &RuleMatcher) -> RuleMatcher {
    RuleMatcher {
        kind: matcher.kind,
        pattern: match matcher.kind {
            RuleMatcherKind::DomainSuffix => normalize_domain(&matcher.pattern),
            RuleMatcherKind::DestinationCidr => matcher.pattern.trim().to_string(),
        },
    }
}
fn outbound(node: &FlowNode) -> String {
    match node.kind {
        FlowNodeKind::Builtin => "direct".into(),
        FlowNodeKind::NodeGroup => dae_identifier(&node.data.name),
        FlowNodeKind::Rule => unreachable!("rules cannot be targets"),
    }
}
fn normalize_domain(value: &str) -> String {
    value.trim().trim_matches('.').to_ascii_lowercase()
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

    #[test]
    fn default_document_contains_one_group_and_fixed_direct() {
        let document = OrchestrationDocument::default();
        assert_eq!(document.version, 2);
        assert_eq!(document.nodes, vec![default_group(), direct_builtin()]);
        assert!(document.edges.is_empty());
        assert!(document.validate().valid);
        assert!(!document.validate().dae_compatible);
        assert!(document
            .validate()
            .issues
            .iter()
            .any(|issue| issue.code == "group_source_required"));
    }

    #[test]
    fn rules_compile_matchers_by_priority_and_fall_back_to_direct() {
        let document = OrchestrationDocument {
            version: 2,
            nodes: vec![
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
            ],
            edges: vec![
                FlowEdge::new("domain-group", "rule-domain", "group"),
                FlowEdge::new("cidr-direct", "rule-cidr", "direct"),
            ],
            viewport: FlowViewport::default(),
        };
        assert_eq!(
            document.compile().unwrap(),
            CompiledRouting {
                conditions: vec![
                    CompiledRoute {
                        matcher: RuleMatcher {
                            kind: RuleMatcherKind::DestinationCidr,
                            pattern: "10.0.0.0/8".into()
                        },
                        priority: 1,
                        condition: "dip(10.0.0.0/8)".into(),
                        outbound: "direct".into()
                    },
                    CompiledRoute {
                        matcher: RuleMatcher {
                            kind: RuleMatcherKind::DomainSuffix,
                            pattern: "example.com".into()
                        },
                        priority: 2,
                        condition: "domain(suffix: example.com)".into(),
                        outbound: "Proxy".into()
                    }
                ],
                fallback: "direct".into()
            }
        );
    }

    #[test]
    fn rejects_invalid_rule_targets_and_non_terminal_groups() {
        let mut document = OrchestrationDocument::default();
        document.nodes.extend([
            rule("rule", RuleMatcherKind::DomainSuffix, "example.com", None),
            group(),
        ]);
        document.edges = vec![
            FlowEdge::new("rule-group", "rule", "group"),
            FlowEdge::new("group-direct", "group", "direct"),
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
            version: 2,
            nodes: vec![first, second, blocked, direct_builtin()],
            edges: vec![
                FlowEdge::new("r1-direct", "r1", "direct"),
                FlowEdge::new("r2-direct", "r2", "direct"),
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
            version: 2,
            nodes: vec![
                rule("rule", RuleMatcherKind::DomainSuffix, "example.com", None),
                group(),
                direct_builtin(),
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
            version: 2,
            nodes: vec![
                rule("rule", RuleMatcherKind::DomainSuffix, "bad..example", None),
                invalid_group,
                direct_builtin(),
            ],
            edges: vec![FlowEdge::new("rule-group", "rule", "group")],
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
            version: 2,
            nodes: vec![
                rule(
                    "rule",
                    RuleMatcherKind::DomainSuffix,
                    "example.com",
                    Some(1),
                ),
                target,
                direct_builtin(),
            ],
            edges: vec![FlowEdge::new("rule-group", "rule", "group")],
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
            version: 2,
            nodes: vec![first, second, direct_builtin()],
            edges: vec![],
            viewport: FlowViewport::default(),
        };
        assert!(document
            .validate()
            .issues
            .iter()
            .any(|issue| issue.code == "group_source_cycle"));
    }
}
