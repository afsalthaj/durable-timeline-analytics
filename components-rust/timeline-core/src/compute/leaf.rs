// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use common_lib::{
    Event, EventColumnName, EventPredicate, EventValue, StateDynamicsTimeLine, StatePoint,
};

use super::to_state_point;
use crate::types::LeafOperation;

/// A state transition a leaf reports to its parent.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafChange {
    pub time: u64,
    pub value: EventValue,
}

/// The state of one leaf node: its operation and the timeline it has built
/// from the events seen so far.
pub struct LeafState {
    operation: LeafOperation,
    latest_event_state: StateDynamicsTimeLine<EventValue>,
    has_existed_state: StateDynamicsTimeLine<bool>,
}

impl LeafState {
    pub fn new(operation: LeafOperation) -> Self {
        LeafState {
            operation,
            latest_event_state: StateDynamicsTimeLine::default(),
            has_existed_state: StateDynamicsTimeLine::default(),
        }
    }

    pub fn operation(&self) -> &LeafOperation {
        &self.operation
    }

    /// Applies `event` and returns the state transitions to report upward.
    pub fn ingest(&mut self, event: &Event) -> Vec<LeafChange> {
        match &self.operation {
            LeafOperation::LatestEventToState(column) => {
                track_latest(&mut self.latest_event_state, column, event)
            }
            LeafOperation::HasExisted(predicate) => {
                track_has_existed(&mut self.has_existed_state, predicate, event, None)
            }
            LeafOperation::HasExistedWithin { predicate, within } => {
                track_has_existed(&mut self.has_existed_state, predicate, event, Some(*within))
            }
        }
    }

    /// The state in effect just before `t1`.
    pub fn state_at(&self, t1: u64) -> Option<StatePoint> {
        match &self.operation {
            LeafOperation::LatestEventToState(_) => {
                to_state_point(self.latest_event_state.get_state_at(t1), EventValue::clone)
            }
            LeafOperation::HasExisted(_) | LeafOperation::HasExistedWithin { .. } => {
                to_state_point(self.has_existed_state.get_state_at(t1), |b| {
                    EventValue::BoolValue(*b)
                })
            }
        }
    }
}

fn track_latest(
    state: &mut StateDynamicsTimeLine<EventValue>,
    column: &EventColumnName,
    event: &Event,
) -> Vec<LeafChange> {
    let Some(value) = event.value_of(column) else {
        return vec![];
    };
    let previous = state.get_state_at(event.time).map(|p| p.value.clone());
    state.add_state_dynamic_info(event.time, value.clone());
    if previous.as_ref() == Some(value) {
        vec![]
    } else {
        vec![LeafChange {
            time: event.time,
            value: value.clone(),
        }]
    }
}

/// `has_existed` latches to `true` on the first match. With `within`, the
/// state reverts to `false` `within` time units after the match.
fn track_has_existed(
    state: &mut StateDynamicsTimeLine<bool>,
    predicate: &EventPredicate,
    event: &Event,
    within: Option<u64>,
) -> Vec<LeafChange> {
    if !(state.is_empty() || state.future_is(false)) {
        return vec![];
    }
    let time = event.time;
    if predicate.evaluate(event) {
        state.add_state_dynamic_info(time, true);
        let mut changes = vec![LeafChange {
            time,
            value: EventValue::BoolValue(true),
        }];
        if let Some(within) = within {
            let expiry = time.saturating_add(within);
            state.add_state_dynamic_info(expiry, false);
            changes.push(LeafChange {
                time: expiry,
                value: EventValue::BoolValue(false),
            });
        }
        changes
    } else if !state.future_is(false) {
        state.add_state_dynamic_info(time, false);
        vec![LeafChange {
            time,
            value: EventValue::BoolValue(false),
        }]
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use common_lib::EventField;

    use super::*;

    fn string(s: &str) -> EventValue {
        EventValue::StringValue(s.to_string())
    }

    fn event(time: u64, column: &str, value: &str) -> Event {
        Event {
            time,
            fields: vec![EventField::new(column, string(value))],
        }
    }

    fn change(time: u64, value: EventValue) -> LeafChange {
        LeafChange { time, value }
    }

    fn latest(column: &str) -> LeafState {
        LeafState::new(LeafOperation::LatestEventToState(EventColumnName::new(
            column,
        )))
    }

    fn is_play() -> EventPredicate {
        EventPredicate::equals("state", string("play"))
    }

    #[test]
    fn latest_reports_only_value_changes() {
        let mut leaf = latest("state");
        assert_eq!(
            leaf.ingest(&event(1, "state", "play")),
            vec![change(1, string("play"))]
        );
        assert_eq!(leaf.ingest(&event(2, "state", "play")), vec![]);
        assert_eq!(
            leaf.ingest(&event(3, "state", "pause")),
            vec![change(3, string("pause"))]
        );
    }

    #[test]
    fn latest_ignores_events_without_its_column() {
        let mut leaf = latest("state");
        assert_eq!(leaf.ingest(&event(1, "cdn", "akamai")), vec![]);
        assert_eq!(leaf.state_at(5), None);
    }

    #[test]
    fn latest_state_at_query_time() {
        let mut leaf = latest("state");
        leaf.ingest(&event(1, "state", "play"));
        leaf.ingest(&event(5, "state", "pause"));
        assert_eq!(
            leaf.state_at(3),
            Some(StatePoint {
                t1: 1,
                t2: Some(5),
                value: string("play"),
            })
        );
    }

    #[test]
    fn has_existed_latches_true() {
        let mut leaf = LeafState::new(LeafOperation::HasExisted(is_play()));
        assert_eq!(
            leaf.ingest(&event(1, "state", "init")),
            vec![change(1, EventValue::BoolValue(false))]
        );
        assert_eq!(
            leaf.ingest(&event(2, "state", "play")),
            vec![change(2, EventValue::BoolValue(true))]
        );
        assert_eq!(leaf.ingest(&event(3, "state", "pause")), vec![]);
        assert_eq!(
            leaf.state_at(10).map(|p| p.value),
            Some(EventValue::BoolValue(true))
        );
    }

    #[test]
    fn has_existed_within_expires() {
        let mut leaf = LeafState::new(LeafOperation::HasExistedWithin {
            predicate: is_play(),
            within: 5,
        });
        assert_eq!(
            leaf.ingest(&event(10, "state", "play")),
            vec![
                change(10, EventValue::BoolValue(true)),
                change(15, EventValue::BoolValue(false)),
            ]
        );
        assert_eq!(
            leaf.state_at(12).map(|p| p.value),
            Some(EventValue::BoolValue(true))
        );
        assert_eq!(
            leaf.state_at(16).map(|p| p.value),
            Some(EventValue::BoolValue(false))
        );
    }
}
