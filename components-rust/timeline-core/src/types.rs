// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Types used only to wire agents together. Values returned to clients live in
//! `common_lib::agent_api`.
use common_lib::{AgentName, CompareOp, EventColumnName, EventPredicate, EventValue};
use golem_rust::{FromWire, IntoWire, Schema, WireSchema};

/// Which leaf computation an event processor performs.
#[derive(Clone, Debug, Schema, IntoWire, FromWire, WireSchema)]
pub enum LeafOperation {
    LatestEventToState(EventColumnName),
    HasExisted(EventPredicate),
    HasExistedWithin {
        predicate: EventPredicate,
        within: u64,
    },
}

/// Which derived computation a timeline processor performs.
#[derive(Clone, Debug, Schema, IntoWire, FromWire, WireSchema)]
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
