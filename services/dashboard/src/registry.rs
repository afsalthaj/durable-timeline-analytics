// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use common_lib::{AggregationConfig, EventColumnName, NumberedNode, TimeLineOp};
use timeline_dsl::{parse, ParsedTimeline};

// ── Newtypes ──

/// A human-readable name for a registered metric (e.g., "cirr", "idle-time").
#[derive(Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct MetricName(pub String);

/// The raw DSL text as written by the developer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DslText(pub String);

/// The agent name suffix, e.g. "has-existed-4".
/// The full agent name is `{session-id}-{suffix}`.
#[derive(Clone, Debug, Serialize, Hash, Eq, PartialEq)]
pub struct AgentSuffix(pub String);

/// Human-readable description of what a node computes.
#[derive(Clone, Debug, Serialize)]
pub struct NodeDescription(pub String);

/// Optional `.describe("...")` caption written on a node.
#[derive(Clone, Debug, Serialize)]
pub struct NodeCaption(pub String);

/// The operation of a derived node, e.g. "and" or "equal-to(buffer)".
#[derive(Clone, Debug, Serialize)]
pub struct OperationName(pub String);

/// Name of an aggregation function (e.g., "count", "sum").
#[derive(Clone, Debug, Serialize)]
pub struct AggregationFunctionName(pub String);

// ── Domain types ──

/// A leaf agent in the compiled tree.
#[derive(Clone, Debug, Serialize)]
pub struct LeafInfo {
    pub agent_suffix: AgentSuffix,
    pub event_columns: Vec<EventColumnName>,
    pub description: NodeDescription,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<NodeCaption>,
}

/// A derived (non-leaf) agent in the compiled tree.
#[derive(Clone, Debug, Serialize)]
pub struct DerivedInfo {
    pub agent_suffix: AgentSuffix,
    pub operation: OperationName,
    pub description: NodeDescription,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<NodeCaption>,
}

/// Maps an event column to the agent suffixes of leaves that need events
/// containing that column.
#[derive(Clone, Debug, Serialize)]
pub struct LeafRoutingTable {
    pub routes: HashMap<EventColumnName, Vec<AgentSuffix>>,
}

/// Aggregation configuration as derived from the DSL.
#[derive(Clone, Debug, Serialize)]
pub struct AggregationInfo {
    pub group_by_column: EventColumnName,
    pub functions: Vec<AggregationFunctionName>,
}

/// A registered metric with its compiled representation and routing info.
#[derive(Clone, Debug, Serialize)]
pub struct RegisteredMetric {
    pub name: MetricName,
    pub dsl: DslText,
    pub leaves: Vec<LeafInfo>,
    pub derived_nodes: Vec<DerivedInfo>,
    pub routing_table: LeafRoutingTable,
    pub aggregation: Option<AggregationInfo>,
}

/// Request body for POST /api/metrics.
#[derive(Deserialize)]
pub struct RegisterMetricRequest {
    pub name: MetricName,
    pub dsl: DslText,
}

/// A registered metric together with what the feeder needs to run it.
#[derive(Clone, Debug)]
pub struct Registration {
    pub metric: RegisteredMetric,
    pub timeline: TimeLineOp,
    pub aggregation: Option<AggregationConfig>,
}

// ── Registry ──

/// In-memory metric registry. Deduplicates by normalized DSL text.
#[derive(Clone, Default)]
pub struct MetricRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

#[derive(Default)]
struct RegistryInner {
    registrations: Vec<Registration>,
    /// Normalized DSL text → index into `registrations`.
    dsl_index: HashMap<String, usize>,
}

impl MetricRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a metric. If an identical DSL is already registered, returns
    /// the existing registration.
    pub fn register(&self, req: &RegisterMetricRequest) -> Result<Registration, String> {
        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;

        let normalized = normalize_dsl(&req.dsl.0);
        if let Some(existing) = inner
            .dsl_index
            .get(&normalized)
            .and_then(|&idx| inner.registrations.get(idx))
        {
            return Ok(existing.clone());
        }

        let parsed = parse(&req.dsl.0).map_err(|e| e.to_string())?;
        let registration = compile(req, parsed);

        let idx = inner.registrations.len();
        inner.dsl_index.insert(normalized, idx);
        inner.registrations.push(registration.clone());
        Ok(registration)
    }

    /// List all registered metrics.
    pub fn list(&self) -> Result<Vec<RegisteredMetric>, String> {
        let inner = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(inner
            .registrations
            .iter()
            .map(|r| r.metric.clone())
            .collect())
    }
}

fn compile(req: &RegisterMetricRequest, parsed: ParsedTimeline) -> Registration {
    let mut leaves = Vec::new();
    let mut derived_nodes = Vec::new();
    for node in parsed.op.numbered_nodes() {
        let agent_suffix = AgentSuffix(node.suffix());
        let caption = parsed.captions.get(&node.number).cloned().map(NodeCaption);
        if node.op.is_leaf() {
            leaves.push(LeafInfo {
                agent_suffix,
                event_columns: node.op.leaf_columns().into_iter().cloned().collect(),
                description: NodeDescription(describe(&node)),
                caption,
            });
        } else {
            derived_nodes.push(DerivedInfo {
                agent_suffix,
                operation: OperationName(operation(node.op)),
                description: NodeDescription(describe(&node)),
                caption,
            });
        }
    }

    let mut routes: HashMap<EventColumnName, Vec<AgentSuffix>> = HashMap::new();
    for leaf in &leaves {
        for column in &leaf.event_columns {
            routes
                .entry(column.clone())
                .or_default()
                .push(leaf.agent_suffix.clone());
        }
    }

    let aggregation = parsed.aggregation.as_ref().map(|config| AggregationInfo {
        group_by_column: config.group_by_column.clone(),
        functions: config
            .functions
            .iter()
            .map(|f| AggregationFunctionName(f.to_string()))
            .collect(),
    });

    Registration {
        metric: RegisteredMetric {
            name: req.name.clone(),
            dsl: req.dsl.clone(),
            leaves,
            derived_nodes,
            routing_table: LeafRoutingTable { routes },
            aggregation,
        },
        timeline: parsed.op,
        aggregation: parsed.aggregation,
    }
}

fn operation(op: &TimeLineOp) -> String {
    match op {
        TimeLineOp::Compare { op, value, .. } => format!("{}({value})", op.operation_name()),
        other => other.operation_name().to_string(),
    }
}

fn describe(node: &NumberedNode<'_>) -> String {
    match node.op {
        TimeLineOp::TlLatestEventToState(column) => {
            format!("Track the latest value of \"{column}\" as state")
        }
        TimeLineOp::TlHasExisted(predicate) => format!("Has {predicate} ever been true?"),
        TimeLineOp::TlHasExistedWithin { predicate, within } => {
            format!("Has {predicate} been true within the last {within} time units?")
        }
        TimeLineOp::Compare { op, value, .. } => {
            let relation = match op {
                common_lib::CompareOp::EqualTo => "equal to",
                common_lib::CompareOp::GreaterThan => "greater than",
                common_lib::CompareOp::GreaterThanOrEqual => "≥",
                common_lib::CompareOp::LessThan => "less than",
                common_lib::CompareOp::LessThanOrEqual => "≤",
            };
            format!("Is the child's state {relation} {value}?")
        }
        TimeLineOp::Not(_) => "Negate the child's boolean state".to_string(),
        TimeLineOp::And { .. } => "Both children must be true".to_string(),
        TimeLineOp::Or { .. } => "Either child must be true".to_string(),
        TimeLineOp::TlDurationWhere(_) => "Cumulative duration where child is true".to_string(),
        TimeLineOp::TlDurationInCurState(_) => "Duration in the child's current state".to_string(),
    }
}

/// Normalize DSL whitespace for deduplication.
fn normalize_dsl(dsl: &str) -> String {
    dsl.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CIRR: &str = r#"duration_where(
        has_existed(playerStateChange == "play")
        && !has_existed_within(playerStateChange == "seek", 5)
        && latest_event_to_state(playerStateChange) == "buffer"
    ) | aggregate(group_by(cdn), count, sum, avg)"#;

    fn request(name: &str, dsl: &str) -> RegisterMetricRequest {
        RegisterMetricRequest {
            name: MetricName(name.to_string()),
            dsl: DslText(dsl.to_string()),
        }
    }

    fn suffixes<'a>(names: impl Iterator<Item = &'a AgentSuffix>) -> Vec<&'a str> {
        names.map(|s| s.0.as_str()).collect()
    }

    #[test]
    fn cirr_compiles_to_driver_agent_names() -> Result<(), String> {
        let registration = MetricRegistry::new().register(&request("cirr", CIRR))?;
        let metric = &registration.metric;
        assert_eq!(
            suffixes(metric.leaves.iter().map(|l| &l.agent_suffix)),
            vec![
                "has-existed-4",
                "has-existed-within-6",
                "latest-event-to-state-8"
            ]
        );
        assert_eq!(
            suffixes(metric.derived_nodes.iter().map(|d| &d.agent_suffix)),
            vec!["duration-where-1", "and-2", "and-3", "not-5", "equal-to-7"]
        );
        let routed = metric
            .routing_table
            .routes
            .get(&EventColumnName::new("playerStateChange"))
            .map(|r| r.len());
        assert_eq!(routed, Some(3));
        assert_eq!(
            metric
                .aggregation
                .as_ref()
                .map(|a| a.group_by_column.as_str()),
            Some("cdn")
        );
        Ok(())
    }

    #[test]
    fn identical_dsl_is_deduplicated() -> Result<(), String> {
        let registry = MetricRegistry::new();
        registry.register(&request("a", "latest_event_to_state(x) == 1"))?;
        let again = registry.register(&request("b", "latest_event_to_state(x)   ==\n 1"))?;
        assert_eq!(again.metric.name, MetricName("a".to_string()));
        assert_eq!(registry.list()?.len(), 1);
        Ok(())
    }

    #[test]
    fn parse_errors_are_reported() {
        assert!(MetricRegistry::new()
            .register(&request("bad", "has_existed("))
            .is_err());
    }

    #[test]
    fn describe_caption_is_kept_on_the_node() -> Result<(), String> {
        let registration = MetricRegistry::new().register(&request(
            "labeled",
            r#"has_existed(playerStateChange == "play").describe("has the user ever played")"#,
        ))?;
        let leaf = registration
            .metric
            .leaves
            .first()
            .ok_or("expected a leaf")?;
        assert_eq!(
            leaf.caption.as_ref().map(|c| c.0.as_str()),
            Some("has the user ever played")
        );
        assert!(leaf.description.0.contains("playerStateChange"));
        Ok(())
    }
}
