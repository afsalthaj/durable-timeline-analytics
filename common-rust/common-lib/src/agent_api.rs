// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Values returned by the timeline agents, shared by the component and its clients.
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

use crate::event::{EventColumnName, EventValue};
use crate::timeline_op::NumberedNode;

/// Identifies one stream of events, e.g. one video playback session. It names
/// the session's TimelineDriver and prefixes all of its node agents.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        SessionId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Name of a registered metric, e.g. `cirr`. Scopes the agents of one
/// metric so several metrics can run on the same session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    ),
    schema(transparent)
)]
pub struct MetricName(pub String);

impl MetricName {
    pub fn new(name: impl Into<String>) -> Self {
        MetricName(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for MetricName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Name of a Golem agent, e.g. `sess-42-cirr-has-existed-4`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    ),
    schema(transparent)
)]
pub struct AgentName(pub String);

impl AgentName {
    pub fn new(name: impl Into<String>) -> Self {
        AgentName(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The agent computing `node` of `metric` for `session`,
    /// e.g. `sess-42-cirr-has-existed-4`.
    pub fn for_node(session: &SessionId, metric: &MetricName, node: &NumberedNode<'_>) -> Self {
        AgentName(format!("{session}-{metric}-{}", node.suffix()))
    }

    /// The aggregator for one group of `metric`, e.g. `aggregator-cirr-cdn-akamai`.
    pub fn for_aggregator(
        metric: &MetricName,
        group_by_column: &EventColumnName,
        group_value: &EventValue,
    ) -> Self {
        AgentName(format!(
            "aggregator-{metric}-{group_by_column}-{group_value}"
        ))
    }
}

impl Display for AgentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The state of a timeline over the interval `[t1, t2)`; `t2 = None` means
/// the state holds into the future.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub struct StatePoint {
    pub t1: u64,
    pub t2: Option<u64>,
    pub value: EventValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub enum TimelineError {
    NotInitialized,
}

impl Display for TimelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimelineError::NotInitialized => f.write_str("timeline node is not initialized"),
        }
    }
}

impl std::error::Error for TimelineError {}

/// Returned by the driver: the root agent and every leaf agent, so a feeder
/// knows where to send events without walking the expression.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub struct InitializeResult {
    pub root_agent: AgentName,
    pub leaf_agents: Vec<AgentName>,
}

/// Aggregated metrics across all sessions of one group. `min` and `max` are
/// `None` when no session has reported yet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub struct AggregationResult {
    pub count: u64,
    pub sum: f64,
    pub avg: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline_op::TimeLineOp;

    #[test]
    fn node_agent_names() {
        let op = !TimeLineOp::TlLatestEventToState(EventColumnName::new("x"));
        let session = SessionId::new("sess-42");
        let names: Vec<AgentName> = op
            .numbered_nodes()
            .iter()
            .map(|node| AgentName::for_node(&session, &MetricName::new("m"), node))
            .collect();
        assert_eq!(
            names,
            vec![
                AgentName::new("sess-42-m-not-1"),
                AgentName::new("sess-42-m-latest-event-to-state-2")
            ]
        );
    }

    #[test]
    fn aggregator_agent_name() {
        let name = AgentName::for_aggregator(
            &MetricName::new("cirr"),
            &EventColumnName::new("cdn"),
            &EventValue::StringValue("akamai".into()),
        );
        assert_eq!(name, AgentName::new("aggregator-cirr-cdn-akamai"));
    }
}
