// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::collections::BTreeSet;

use common_lib::{EventValue, StateDynamicsTimeLine, StatePoint};

use super::to_state_point;
use crate::types::{ChildSlot, DerivedOperation, SessionValue};

/// A state change from one of a derived node's children.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildUpdate {
    pub slot: ChildSlot,
    pub time: u64,
    pub value: EventValue,
}

/// A state change this node reports to its parent. `time` can be later than
/// the child update that produced it, when a boundary already stored on the
/// other child changes the combination.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedChange {
    pub time: u64,
    pub value: EventValue,
}

/// Running state of a duration node.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DurationState {
    /// Counting up since `since`, starting from `base`.
    Climbing { base: u64, since: u64 },
    /// Holding at `value`.
    Flat { value: u64 },
}

impl DurationState {
    fn value_at(self, t: u64) -> u64 {
        match self {
            DurationState::Climbing { base, since } => base.saturating_add(t.saturating_sub(since)),
            DurationState::Flat { value } => value,
        }
    }
}

/// One stretch of a duration, closed when the next stretch starts.
#[derive(Clone, Copy, Debug)]
struct DurationSegment {
    t1: u64,
    t2: Option<u64>,
    state: DurationState,
}

/// The state of one derived node: its operation, the latest state of its
/// children, and its own result timeline.
pub struct DerivedState {
    operation: DerivedOperation,
    left_child_state: StateDynamicsTimeLine<EventValue>,
    right_child_state: StateDynamicsTimeLine<EventValue>,
    result_state: StateDynamicsTimeLine<EventValue>,
    /// Climbing and flat stretches of a duration node, oldest first.
    duration_segments: Vec<DurationSegment>,
}

impl DerivedState {
    pub fn new(operation: DerivedOperation) -> Self {
        DerivedState {
            operation,
            left_child_state: StateDynamicsTimeLine::default(),
            right_child_state: StateDynamicsTimeLine::default(),
            result_state: StateDynamicsTimeLine::default(),
            duration_segments: Vec::new(),
        }
    }

    pub fn operation(&self) -> &DerivedOperation {
        &self.operation
    }

    /// Updates this node's state for a child change. Returns every value to
    /// push upward, in time order. Empty when nothing observable changed.
    ///
    /// Boolean combinations are re-evaluated at every later boundary already
    /// stored on either child. A seek window that expires after buffering has
    /// started is one of those boundaries: the expiry was recorded when the
    /// seek arrived, and the buffer change has to notice it.
    pub fn apply(&mut self, update: ChildUpdate) -> Vec<DerivedChange> {
        let ChildUpdate { slot, time, value } = update;
        match &self.operation {
            DerivedOperation::Compare {
                op,
                value: constant,
            } => {
                let result = EventValue::BoolValue(op.apply(&value, constant));
                emit(time, self.record_if_changed(time, result))
            }
            DerivedOperation::Not => {
                let Some(b) = value.as_bool() else {
                    return Vec::new();
                };
                emit(
                    time,
                    self.record_if_changed(time, EventValue::BoolValue(!b)),
                )
            }
            DerivedOperation::And => self.combine(slot, time, value, |l, r| l && r),
            DerivedOperation::Or => self.combine(slot, time, value, |l, r| l || r),
            DerivedOperation::DurationWhere => {
                let Some(b) = value.as_bool() else {
                    return Vec::new();
                };
                let current = self.duration_at(time).unwrap_or(0);
                let next = if b {
                    DurationState::Climbing {
                        base: current,
                        since: time,
                    }
                } else {
                    DurationState::Flat { value: current }
                };
                let recorded = self.start_segment(time, next);
                vec![DerivedChange {
                    time,
                    value: duration_value(recorded),
                }]
            }
            DerivedOperation::DurationInCurState => {
                let recorded = self.start_segment(
                    time,
                    DurationState::Climbing {
                        base: 0,
                        since: time,
                    },
                );
                vec![DerivedChange {
                    time,
                    value: duration_value(recorded),
                }]
            }
        }
    }

    /// The state in effect at `t1`. Duration nodes report the duration that
    /// had accumulated by `t1`, including stretches that have since ended.
    /// Other nodes report the segment that holds at `t1`.
    pub fn state_at(&self, t1: u64) -> Option<StatePoint> {
        if !self.duration_segments.is_empty() {
            return self.duration_at(t1).map(|value| StatePoint {
                t1,
                t2: None,
                value: duration_value(value),
            });
        }
        to_state_point(self.result_state.state_holding_at(t1), EventValue::clone)
    }

    /// This node's latest value as a contribution to an aggregate: rising
    /// while a duration is accumulating, fixed otherwise. `None` if the node
    /// has no numeric value yet.
    pub fn session_value(&self) -> Option<SessionValue> {
        match self.duration_segments.last().map(|segment| segment.state) {
            Some(DurationState::Climbing { base, since }) => Some(SessionValue::Rising {
                base: base as f64,
                since,
            }),
            Some(DurationState::Flat { value }) => Some(SessionValue::Fixed {
                value: value as f64,
            }),
            None => self
                .result_state
                .last()
                .and_then(|point| point.value.as_f64())
                .map(|value| SessionValue::Fixed { value }),
        }
    }

    fn combine(
        &mut self,
        slot: ChildSlot,
        time: u64,
        value: EventValue,
        f: impl Fn(bool, bool) -> bool,
    ) -> Vec<DerivedChange> {
        match slot {
            ChildSlot::Left => self.left_child_state.add_state_dynamic_info(time, value),
            ChildSlot::Right => self.right_child_state.add_state_dynamic_info(time, value),
        }
        let rebuilt = combine_timelines(&self.left_child_state, &self.right_child_state, f);
        let changes = changed_points(&self.result_state, &rebuilt);
        self.result_state = rebuilt;
        changes
    }

    fn start_segment(&mut self, time: u64, next: DurationState) -> u64 {
        if self
            .duration_segments
            .last()
            .is_some_and(|segment| segment.t1 == time)
        {
            self.duration_segments.pop();
        } else if let Some(open) = self.duration_segments.last_mut() {
            open.t2 = Some(time);
        }
        let value_now = next.value_at(time);
        self.duration_segments.push(DurationSegment {
            t1: time,
            t2: None,
            state: next,
        });
        self.record(time, duration_value(value_now));
        value_now
    }

    fn duration_at(&self, t: u64) -> Option<u64> {
        self.duration_segments
            .iter()
            .rev()
            .find(|segment| segment.t1 <= t && segment.t2.is_none_or(|end| t < end))
            .map(|segment| segment.state.value_at(t))
    }

    fn record_if_changed(&mut self, time: u64, value: EventValue) -> Option<EventValue> {
        let previous = self.result_state.get_state_at(time).map(|p| &p.value);
        if previous == Some(&value) {
            None
        } else {
            Some(self.record(time, value))
        }
    }

    fn record(&mut self, time: u64, value: EventValue) -> EventValue {
        self.result_state
            .add_state_dynamic_info(time, value.clone());
        value
    }
}

fn duration_value(duration: u64) -> EventValue {
    EventValue::IntValue(i64::try_from(duration).unwrap_or(i64::MAX))
}

fn emit(time: u64, value: Option<EventValue>) -> Vec<DerivedChange> {
    value
        .into_iter()
        .map(|value| DerivedChange { time, value })
        .collect()
}

fn bool_holding(state: &StateDynamicsTimeLine<EventValue>, time: u64) -> Option<bool> {
    state
        .state_holding_at(time)
        .and_then(|point| point.value.as_bool())
}

/// Boolean combination of two child timelines. A time where either child has
/// not started yet produces no output.
fn combine_timelines(
    left: &StateDynamicsTimeLine<EventValue>,
    right: &StateDynamicsTimeLine<EventValue>,
    f: impl Fn(bool, bool) -> bool,
) -> StateDynamicsTimeLine<EventValue> {
    let mut times = BTreeSet::new();
    times.extend(left.points.keys().copied());
    times.extend(right.points.keys().copied());
    let mut result = StateDynamicsTimeLine::default();
    let mut last = None;
    for time in times {
        let Some(left_value) = bool_holding(left, time) else {
            continue;
        };
        let Some(right_value) = bool_holding(right, time) else {
            continue;
        };
        let value = f(left_value, right_value);
        if last != Some(value) {
            result.add_state_dynamic_info(time, EventValue::BoolValue(value));
            last = Some(value);
        }
    }
    result
}

/// Points where the boolean holding at that time changed.
fn changed_points(
    previous: &StateDynamicsTimeLine<EventValue>,
    updated: &StateDynamicsTimeLine<EventValue>,
) -> Vec<DerivedChange> {
    let mut times = BTreeSet::new();
    times.extend(previous.points.keys().copied());
    times.extend(updated.points.keys().copied());
    times
        .into_iter()
        .filter_map(|time| {
            let before = bool_holding(previous, time);
            let after = bool_holding(updated, time);
            if before == after {
                None
            } else {
                after.map(|value| DerivedChange {
                    time,
                    value: EventValue::BoolValue(value),
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use common_lib::CompareOp;

    use super::*;

    fn update(slot: ChildSlot, time: u64, value: EventValue) -> ChildUpdate {
        ChildUpdate { slot, time, value }
    }

    fn left(time: u64, value: bool) -> ChildUpdate {
        update(ChildSlot::Left, time, EventValue::BoolValue(value))
    }

    fn right(time: u64, value: bool) -> ChildUpdate {
        update(ChildSlot::Right, time, EventValue::BoolValue(value))
    }

    fn at(time: u64, value: EventValue) -> Vec<DerivedChange> {
        vec![DerivedChange { time, value }]
    }

    fn bool_at(time: u64, value: bool) -> Vec<DerivedChange> {
        at(time, EventValue::BoolValue(value))
    }

    fn int_at(node: &DerivedState, t: u64) -> Option<EventValue> {
        node.state_at(t).map(|p| p.value)
    }

    #[test]
    fn compare_reports_only_changes() {
        let mut node = DerivedState::new(DerivedOperation::Compare {
            op: CompareOp::EqualTo,
            value: EventValue::StringValue("buffer".into()),
        });
        let state =
            |time, s: &str| update(ChildSlot::Left, time, EventValue::StringValue(s.into()));
        assert_eq!(node.apply(state(1, "play")), bool_at(1, false));
        assert_eq!(node.apply(state(2, "pause")), vec![]);
        assert_eq!(node.apply(state(3, "buffer")), bool_at(3, true));
    }

    #[test]
    fn not_negates_and_ignores_non_booleans() {
        let mut node = DerivedState::new(DerivedOperation::Not);
        assert_eq!(node.apply(left(1, false)), bool_at(1, true));
        assert_eq!(
            node.apply(update(ChildSlot::Left, 2, EventValue::IntValue(1))),
            vec![]
        );
    }

    #[test]
    fn and_waits_for_both_children() {
        let mut node = DerivedState::new(DerivedOperation::And);
        assert_eq!(node.apply(left(1, true)), vec![]);
        assert_eq!(node.apply(right(2, true)), bool_at(2, true));
        assert_eq!(node.apply(left(3, false)), bool_at(3, false));
        assert_eq!(node.apply(right(4, false)), vec![]);
    }

    #[test]
    fn or_is_true_if_either_child_is() {
        let mut node = DerivedState::new(DerivedOperation::Or);
        node.apply(left(1, false));
        assert_eq!(node.apply(right(1, false)), bool_at(1, false));
        assert_eq!(node.apply(right(2, true)), bool_at(2, true));
    }

    /// Buffering that starts while a seek window is still open must count
    /// from the moment the window expires, not stay false forever.
    #[test]
    fn and_rechecks_a_later_boundary_on_the_other_child() {
        let mut node = DerivedState::new(DerivedOperation::And);
        node.apply(left(100, false));
        node.apply(left(105, true));
        node.apply(right(100, false));
        node.apply(right(102, true));
        assert_eq!(
            node.state_at(104).map(|point| point.value),
            Some(EventValue::BoolValue(false))
        );
        assert_eq!(
            node.state_at(105).map(|point| point.value),
            Some(EventValue::BoolValue(true))
        );
    }

    /// After the condition stops, a query inside the earlier climbing window
    /// is the duration then (50 at t=250), not the total accumulated since (100).
    #[test]
    fn duration_where_past_query_returns_the_value_at_that_time() {
        let mut node = DerivedState::new(DerivedOperation::DurationWhere);
        node.apply(left(200, true));
        node.apply(left(300, false));
        assert_eq!(int_at(&node, 250), Some(EventValue::IntValue(50)));
        assert_eq!(int_at(&node, 300), Some(EventValue::IntValue(100)));
        assert_eq!(int_at(&node, 400), Some(EventValue::IntValue(100)));
    }

    #[test]
    fn duration_in_cur_state_past_query_keeps_the_previous_state() {
        let mut node = DerivedState::new(DerivedOperation::DurationInCurState);
        node.apply(update(
            ChildSlot::Left,
            10,
            EventValue::StringValue("a".into()),
        ));
        node.apply(update(
            ChildSlot::Left,
            20,
            EventValue::StringValue("b".into()),
        ));
        assert_eq!(int_at(&node, 15), Some(EventValue::IntValue(5)));
        assert_eq!(int_at(&node, 22), Some(EventValue::IntValue(2)));
    }

    #[test]
    fn duration_where_accumulates_while_true() {
        let mut node = DerivedState::new(DerivedOperation::DurationWhere);
        assert_eq!(
            node.apply(left(100, true)),
            at(100, EventValue::IntValue(0))
        );
        assert_eq!(int_at(&node, 150), Some(EventValue::IntValue(50)));
        assert_eq!(
            node.apply(left(200, false)),
            at(200, EventValue::IntValue(100))
        );
        assert_eq!(int_at(&node, 300), Some(EventValue::IntValue(100)));
        node.apply(left(300, true));
        assert_eq!(int_at(&node, 310), Some(EventValue::IntValue(110)));
    }

    #[test]
    fn duration_in_cur_state_restarts_on_change() {
        let mut node = DerivedState::new(DerivedOperation::DurationInCurState);
        node.apply(update(
            ChildSlot::Left,
            10,
            EventValue::StringValue("a".into()),
        ));
        assert_eq!(int_at(&node, 15), Some(EventValue::IntValue(5)));
        node.apply(update(
            ChildSlot::Left,
            20,
            EventValue::StringValue("b".into()),
        ));
        assert_eq!(int_at(&node, 22), Some(EventValue::IntValue(2)));
    }

    #[test]
    fn duration_session_value_rises_while_true() {
        let mut node = DerivedState::new(DerivedOperation::DurationWhere);
        node.apply(left(100, true));
        assert_eq!(
            node.session_value(),
            Some(SessionValue::Rising {
                base: 0.0,
                since: 100
            })
        );
        node.apply(left(130, false));
        assert_eq!(
            node.session_value(),
            Some(SessionValue::Fixed { value: 30.0 })
        );
    }

    #[test]
    fn boolean_session_value_is_zero_or_one() {
        let mut node = DerivedState::new(DerivedOperation::Not);
        assert_eq!(node.session_value(), None);
        node.apply(left(1, false));
        assert_eq!(
            node.session_value(),
            Some(SessionValue::Fixed { value: 1.0 })
        );
    }

    #[test]
    fn empty_node_has_no_state() {
        let node = DerivedState::new(DerivedOperation::And);
        assert_eq!(node.state_at(10), None);
    }
}
