// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Types used only to wire agents together. Values returned to clients live in
//! `common_lib::agent_api`.
use common_lib::{
    AgentName, AggregationConfig, CompareOp, EventColumnName, EventPredicate, EventValue,
    MetricName,
};
use golem_rust::{FromWire, IntoWire, Schema, WireSchema};

/// Which leaf computation an event processor performs.
#[derive(Clone, Debug, PartialEq, Schema, IntoWire, FromWire, WireSchema)]
pub enum LeafOperation {
    LatestEventToState(EventColumnName),
    HasExisted(EventPredicate),
    HasExistedWithin {
        predicate: EventPredicate,
        within: u64,
    },
}

/// Which derived computation a timeline processor performs.
#[derive(Clone, Debug, PartialEq, Schema, IntoWire, FromWire, WireSchema)]
pub enum DerivedOperation {
    Compare { op: CompareOp, value: EventValue },
    Not,
    And,
    Or,
    DurationWhere,
    DurationInCurState,
}

/// Which input of a derived node a child feeds. Unary nodes only use `Left`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, IntoWire, FromWire, WireSchema)]
pub enum ChildSlot {
    Left,
    Right,
}

/// Where an agent pushes its state changes.
#[derive(Clone, Debug, PartialEq, Eq, Schema, IntoWire, FromWire, WireSchema)]
pub struct ParentRef {
    pub agent_name: AgentName,
    pub slot: ChildSlot,
}

/// Where a root node reports its value: the aggregators of `metric`,
/// one per value of `config.group_by_column`.
#[derive(Clone, Debug, PartialEq, Schema, IntoWire, FromWire, WireSchema)]
pub struct AggregationTarget {
    pub metric: MetricName,
    pub config: AggregationConfig,
}

/// One session's contribution to an aggregate, as a function of time.
#[derive(Clone, Copy, Debug, PartialEq, Schema, IntoWire, FromWire, WireSchema)]
pub enum SessionValue {
    /// Holds at `value`.
    Fixed { value: f64 },
    /// Grows by one per time unit from `base` at `since`, e.g. a duration
    /// that is still accumulating.
    Rising { base: f64, since: u64 },
}

impl SessionValue {
    pub fn value_at(self, t: u64) -> f64 {
        match self {
            SessionValue::Fixed { value } => value,
            SessionValue::Rising { base, since } => base + t.saturating_sub(since) as f64,
        }
    }
}

/// A root node's report of its session's current value to an aggregator.
#[derive(Clone, Debug, PartialEq, Schema, IntoWire, FromWire, WireSchema)]
pub struct SessionReport {
    /// The reporting root agent; identifies the session within the aggregator.
    pub session: AgentName,
    /// The event time of the change that triggered the report.
    pub time: u64,
    pub value: SessionValue,
}

/// A state change pushed from a child agent to its parent.
#[derive(Clone, Debug, Schema, IntoWire, FromWire, WireSchema)]
pub struct ChildStateChange {
    pub slot: ChildSlot,
    pub time: u64,
    pub value: EventValue,
    /// Value of the aggregation group-by column (e.g. `"akamai"`), extracted by
    /// the leaf and carried up to the root.
    pub group_by_value: Option<EventValue>,
}
