// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

use crate::event::{EventColumnName, EventValue};
use crate::event_predicate::EventPredicate;

/// How a timeline's state is compared against a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub enum CompareOp {
    EqualTo,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

impl CompareOp {
    pub fn apply(self, state: &EventValue, constant: &EventValue) -> bool {
        match self {
            CompareOp::EqualTo => state == constant,
            CompareOp::GreaterThan => state > constant,
            CompareOp::GreaterThanOrEqual => state >= constant,
            CompareOp::LessThan => state < constant,
            CompareOp::LessThanOrEqual => state <= constant,
        }
    }

    /// Kebab-case name used in agent names, e.g. `equal-to`.
    pub fn operation_name(self) -> &'static str {
        match self {
            CompareOp::EqualTo => "equal-to",
            CompareOp::GreaterThan => "greater-than",
            CompareOp::GreaterThanOrEqual => "greater-than-or-equal",
            CompareOp::LessThan => "less-than",
            CompareOp::LessThanOrEqual => "less-than-or-equal",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            CompareOp::EqualTo => "EqualTo",
            CompareOp::GreaterThan => "GreaterThan",
            CompareOp::GreaterThanOrEqual => "GreaterThanOrEqual",
            CompareOp::LessThan => "LessThan",
            CompareOp::LessThanOrEqual => "LessThanOrEqual",
        }
    }
}

/// The recursive Timeline DSL expression.
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
pub enum TimeLineOp {
    Compare {
        op: CompareOp,
        timeline: Box<TimeLineOp>,
        value: EventValue,
    },
    And {
        left: Box<TimeLineOp>,
        right: Box<TimeLineOp>,
    },
    Or {
        left: Box<TimeLineOp>,
        right: Box<TimeLineOp>,
    },
    Not(Box<TimeLineOp>),
    TlHasExisted(EventPredicate),
    TlHasExistedWithin {
        predicate: EventPredicate,
        within: u64,
    },
    TlLatestEventToState(EventColumnName),
    TlDurationWhere(Box<TimeLineOp>),
    TlDurationInCurState(Box<TimeLineOp>),
}

/// Position of a node in pre-order depth-first traversal, starting at 1.
/// Together with the operation name it forms the node's agent-name suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeNumber(pub u64);

impl Display for NodeNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A node of a timeline expression, numbered in pre-order.
#[derive(Clone, Debug)]
pub struct NumberedNode<'a> {
    pub number: NodeNumber,
    pub op: &'a TimeLineOp,
    pub children: Vec<NodeNumber>,
}

impl NumberedNode<'_> {
    /// The agent-name suffix of this node, e.g. `has-existed-4`.
    pub fn suffix(&self) -> String {
        format!("{}-{}", self.op.operation_name(), self.number)
    }
}

impl std::ops::Not for TimeLineOp {
    type Output = TimeLineOp;

    fn not(self) -> TimeLineOp {
        TimeLineOp::Not(Box::new(self))
    }
}

impl TimeLineOp {
    pub fn compare(op: CompareOp, timeline: TimeLineOp, value: EventValue) -> Self {
        TimeLineOp::Compare {
            op,
            timeline: Box::new(timeline),
            value,
        }
    }

    pub fn and(left: TimeLineOp, right: TimeLineOp) -> Self {
        TimeLineOp::And {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub fn or(left: TimeLineOp, right: TimeLineOp) -> Self {
        TimeLineOp::Or {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub fn duration_where(timeline: TimeLineOp) -> Self {
        TimeLineOp::TlDurationWhere(Box::new(timeline))
    }

    pub fn duration_in_cur_state(timeline: TimeLineOp) -> Self {
        TimeLineOp::TlDurationInCurState(Box::new(timeline))
    }

    /// Kebab-case operation name used in agent names.
    pub fn operation_name(&self) -> &'static str {
        match self {
            TimeLineOp::Compare { op, .. } => op.operation_name(),
            TimeLineOp::And { .. } => "and",
            TimeLineOp::Or { .. } => "or",
            TimeLineOp::Not(_) => "not",
            TimeLineOp::TlHasExisted(_) => "has-existed",
            TimeLineOp::TlHasExistedWithin { .. } => "has-existed-within",
            TimeLineOp::TlLatestEventToState(_) => "latest-event-to-state",
            TimeLineOp::TlDurationWhere(_) => "duration-where",
            TimeLineOp::TlDurationInCurState(_) => "duration-in-cur-state",
        }
    }

    pub fn children(&self) -> Vec<&TimeLineOp> {
        match self {
            TimeLineOp::Compare { timeline, .. }
            | TimeLineOp::Not(timeline)
            | TimeLineOp::TlDurationWhere(timeline)
            | TimeLineOp::TlDurationInCurState(timeline) => vec![timeline],
            TimeLineOp::And { left, right } | TimeLineOp::Or { left, right } => {
                vec![left, right]
            }
            TimeLineOp::TlHasExisted(_)
            | TimeLineOp::TlHasExistedWithin { .. }
            | TimeLineOp::TlLatestEventToState(_) => vec![],
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.children().is_empty()
    }

    /// Event columns a leaf reads. Empty for derived nodes.
    pub fn leaf_columns(&self) -> Vec<&EventColumnName> {
        match self {
            TimeLineOp::TlHasExisted(predicate)
            | TimeLineOp::TlHasExistedWithin { predicate, .. } => predicate.columns(),
            TimeLineOp::TlLatestEventToState(column) => vec![column],
            _ => vec![],
        }
    }

    /// All nodes in pre-order depth-first order, numbered from 1.
    /// The root is always `NodeNumber(1)`.
    pub fn numbered_nodes(&self) -> Vec<NumberedNode<'_>> {
        let mut nodes = Vec::new();
        self.number_into(&mut nodes);
        nodes
    }

    fn number_into<'a>(&'a self, nodes: &mut Vec<NumberedNode<'a>>) -> NodeNumber {
        let number = NodeNumber(nodes.len() as u64 + 1);
        let position = nodes.len();
        nodes.push(NumberedNode {
            number,
            op: self,
            children: Vec::new(),
        });
        let children: Vec<NodeNumber> = self
            .children()
            .into_iter()
            .map(|child| child.number_into(nodes))
            .collect();
        if let Some(node) = nodes.get_mut(position) {
            node.children = children;
        }
        number
    }
}

impl Display for TimeLineOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeLineOp::Compare {
                op,
                timeline,
                value,
            } => write!(f, "{}({timeline}, {value})", op.display_name()),
            TimeLineOp::And { left, right } => write!(f, "And({left}, {right})"),
            TimeLineOp::Or { left, right } => write!(f, "Or({left}, {right})"),
            TimeLineOp::Not(timeline) => write!(f, "Not({timeline})"),
            TimeLineOp::TlHasExisted(predicate) => write!(f, "TlHasExisted({predicate})"),
            TimeLineOp::TlHasExistedWithin { predicate, within } => {
                write!(f, "TlHasExistedWithin({predicate}, {within})")
            }
            TimeLineOp::TlLatestEventToState(column) => {
                write!(f, "TlLatestEventToState({column})")
            }
            TimeLineOp::TlDurationWhere(timeline) => write!(f, "TlDurationWhere({timeline})"),
            TimeLineOp::TlDurationInCurState(timeline) => {
                write!(f, "TlDurationInCurState({timeline})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play() -> EventPredicate {
        EventPredicate::equals("playerStateChange", EventValue::StringValue("play".into()))
    }

    fn seek() -> EventPredicate {
        EventPredicate::equals("playerStateChange", EventValue::StringValue("seek".into()))
    }

    fn cirr() -> TimeLineOp {
        TimeLineOp::duration_where(TimeLineOp::and(
            TimeLineOp::and(
                TimeLineOp::TlHasExisted(play()),
                !(TimeLineOp::TlHasExistedWithin {
                    predicate: seek(),
                    within: 5,
                }),
            ),
            TimeLineOp::compare(
                CompareOp::EqualTo,
                TimeLineOp::TlLatestEventToState(EventColumnName::new("playerStateChange")),
                EventValue::StringValue("buffer".into()),
            ),
        ))
    }

    #[test]
    fn cirr_numbering_matches_documented_agent_names() {
        let suffixes: Vec<String> = cirr().numbered_nodes().iter().map(|n| n.suffix()).collect();
        assert_eq!(
            suffixes,
            vec![
                "duration-where-1",
                "and-2",
                "and-3",
                "has-existed-4",
                "not-5",
                "has-existed-within-6",
                "equal-to-7",
                "latest-event-to-state-8",
            ]
        );
    }

    #[test]
    fn numbered_nodes_record_children() {
        let op = cirr();
        let nodes = op.numbered_nodes();
        let children: Vec<Vec<u64>> = nodes
            .iter()
            .map(|n| n.children.iter().map(|c| c.0).collect())
            .collect();
        assert_eq!(
            children,
            vec![
                vec![2],
                vec![3, 7],
                vec![4, 5],
                vec![],
                vec![6],
                vec![],
                vec![8],
                vec![]
            ]
        );
    }

    #[test]
    fn leaf_columns() {
        let op = TimeLineOp::TlHasExisted(play().and(EventPredicate::equals(
            "cdn",
            EventValue::StringValue("x".into()),
        )));
        let columns: Vec<&str> = op.leaf_columns().iter().map(|c| c.as_str()).collect();
        assert_eq!(columns, vec!["playerStateChange", "cdn"]);
        assert!((!op).leaf_columns().is_empty());
    }

    #[test]
    fn compare_ops() {
        let one = EventValue::IntValue(1);
        let two = EventValue::IntValue(2);
        assert!(CompareOp::EqualTo.apply(&one, &one));
        assert!(CompareOp::GreaterThan.apply(&two, &one));
        assert!(CompareOp::GreaterThanOrEqual.apply(&one, &one));
        assert!(CompareOp::LessThan.apply(&one, &two));
        assert!(CompareOp::LessThanOrEqual.apply(&two, &two));
        assert!(!CompareOp::LessThan.apply(&two, &one));
    }

    #[test]
    fn display() {
        assert_eq!(
            cirr().to_string(),
            "TlDurationWhere(And(And(TlHasExisted(playerStateChange == play), \
             Not(TlHasExistedWithin(playerStateChange == seek, 5))), \
             EqualTo(TlLatestEventToState(playerStateChange), buffer)))"
        );
    }
}
