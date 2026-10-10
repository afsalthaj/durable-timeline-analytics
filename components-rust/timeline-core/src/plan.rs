// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use common_lib::{AgentName, NodeNumber, SessionId, TimeLineOp};

use crate::types::*;

/// What one agent of a timeline graph computes.
#[derive(Clone, Debug)]
pub enum NodeRole {
    Leaf(LeafOperation),
    Derived(DerivedOperation),
}

/// One agent to create when a timeline is initialized.
#[derive(Clone, Debug)]
pub struct AgentPlan {
    pub agent_name: AgentName,
    pub role: NodeRole,
    /// `None` for the root.
    pub parent: Option<ParentRef>,
}

impl AgentPlan {
    pub fn is_leaf(&self) -> bool {
        matches!(self.role, NodeRole::Leaf(_))
    }
}

/// Agents for a timeline in pre-order (root first), named
/// `{session}-{operation}-{n}`, e.g. `sess-42-has-existed-4`.
pub fn plan_agents(session: &SessionId, timeline: &TimeLineOp) -> Vec<AgentPlan> {
    let nodes = timeline.numbered_nodes();
    let names: Vec<AgentName> = nodes
        .iter()
        .map(|node| AgentName::for_node(session, node))
        .collect();
    let mut parents: Vec<Option<ParentRef>> = vec![None; nodes.len()];
    for node in &nodes {
        let Some(parent_name) = index_of(node.number).and_then(|i| names.get(i)) else {
            continue;
        };
        for (child, slot) in node
            .children
            .iter()
            .zip([ChildSlot::Left, ChildSlot::Right])
        {
            if let Some(entry) = index_of(*child).and_then(|i| parents.get_mut(i)) {
                *entry = Some(ParentRef {
                    agent_name: parent_name.clone(),
                    slot,
                });
            }
        }
    }

    nodes
        .iter()
        .zip(names.iter())
        .zip(parents)
        .map(|((node, agent_name), parent)| AgentPlan {
            agent_name: agent_name.clone(),
            role: role_of(node.op),
            parent,
        })
        .collect()
}

/// Position of a node in the pre-order list; node numbers start at 1.
fn index_of(number: NodeNumber) -> Option<usize> {
    number
        .0
        .checked_sub(1)
        .and_then(|i| usize::try_from(i).ok())
}

fn role_of(op: &TimeLineOp) -> NodeRole {
    match op {
        TimeLineOp::TlLatestEventToState(column) => {
            NodeRole::Leaf(LeafOperation::LatestEventToState(column.clone()))
        }
        TimeLineOp::TlHasExisted(predicate) => {
            NodeRole::Leaf(LeafOperation::HasExisted(predicate.clone()))
        }
        TimeLineOp::TlHasExistedWithin { predicate, within } => {
            NodeRole::Leaf(LeafOperation::HasExistedWithin {
                predicate: predicate.clone(),
                within: *within,
            })
        }
        TimeLineOp::Compare { op, value, .. } => NodeRole::Derived(DerivedOperation::Compare {
            op: *op,
            value: value.clone(),
        }),
        TimeLineOp::Not(_) => NodeRole::Derived(DerivedOperation::Not),
        TimeLineOp::And { .. } => NodeRole::Derived(DerivedOperation::And),
        TimeLineOp::Or { .. } => NodeRole::Derived(DerivedOperation::Or),
        TimeLineOp::TlDurationWhere(_) => NodeRole::Derived(DerivedOperation::DurationWhere),
        TimeLineOp::TlDurationInCurState(_) => {
            NodeRole::Derived(DerivedOperation::DurationInCurState)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_lib::{CompareOp, EventColumnName, EventPredicate, EventValue};

    fn cirr() -> TimeLineOp {
        let state = |s: &str| EventValue::StringValue(s.to_string());
        TimeLineOp::duration_where(TimeLineOp::and(
            TimeLineOp::and(
                TimeLineOp::TlHasExisted(EventPredicate::equals(
                    "playerStateChange",
                    state("play"),
                )),
                !(TimeLineOp::TlHasExistedWithin {
                    predicate: EventPredicate::equals("playerStateChange", state("seek")),
                    within: 5,
                }),
            ),
            TimeLineOp::compare(
                CompareOp::EqualTo,
                TimeLineOp::TlLatestEventToState(EventColumnName::new("playerStateChange")),
                state("buffer"),
            ),
        ))
    }

    #[test]
    fn cirr_plan_names_and_wiring() {
        let plans = plan_agents(&SessionId::new("sess-42"), &cirr());
        let names: Vec<&str> = plans.iter().map(|p| p.agent_name.0.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "sess-42-duration-where-1",
                "sess-42-and-2",
                "sess-42-and-3",
                "sess-42-has-existed-4",
                "sess-42-not-5",
                "sess-42-has-existed-within-6",
                "sess-42-equal-to-7",
                "sess-42-latest-event-to-state-8",
            ]
        );

        let parent = |name: &str, slot| {
            Some(ParentRef {
                agent_name: AgentName::new(format!("sess-42-{name}")),
                slot,
            })
        };
        let parents: Vec<Option<ParentRef>> = plans.iter().map(|p| p.parent.clone()).collect();
        assert_eq!(
            parents,
            vec![
                None,
                parent("duration-where-1", ChildSlot::Left),
                parent("and-2", ChildSlot::Left),
                parent("and-3", ChildSlot::Left),
                parent("and-3", ChildSlot::Right),
                parent("not-5", ChildSlot::Left),
                parent("and-2", ChildSlot::Right),
                parent("equal-to-7", ChildSlot::Left),
            ]
        );

        let leaves: Vec<bool> = plans.iter().map(AgentPlan::is_leaf).collect();
        assert_eq!(
            leaves,
            vec![false, false, false, true, false, true, false, true]
        );
    }

    #[test]
    fn single_leaf_is_its_own_root() {
        let plans = plan_agents(
            &SessionId::new("s"),
            &TimeLineOp::TlLatestEventToState(EventColumnName::new("x")),
        );
        assert_eq!(plans.len(), 1);
        assert!(plans.iter().all(|p| p.is_leaf() && p.parent.is_none()));
    }
}
