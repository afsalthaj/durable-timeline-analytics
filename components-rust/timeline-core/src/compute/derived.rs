// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use common_lib::{EventValue, StateDynamicsTimeLine, StatePoint};

use super::to_state_point;
use crate::types::{ChildSlot, DerivedOperation};

/// A state change from one of a derived node's children.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildUpdate {
    pub slot: ChildSlot,
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

/// The state of one derived node: its operation, the latest state of its
/// children, and its own result timeline.
pub struct DerivedState {
    operation: DerivedOperation,
    left_child_state: StateDynamicsTimeLine<EventValue>,
    right_child_state: StateDynamicsTimeLine<EventValue>,
    result_state: StateDynamicsTimeLine<EventValue>,
    duration_state: Option<DurationState>,
}

impl DerivedState {
    pub fn new(operation: DerivedOperation) -> Self {
        DerivedState {
            operation,
            left_child_state: StateDynamicsTimeLine::default(),
            right_child_state: StateDynamicsTimeLine::default(),
            result_state: StateDynamicsTimeLine::default(),
            duration_state: None,
        }
    }

    pub fn operation(&self) -> &DerivedOperation {
        &self.operation
    }

    /// Updates this node's state for a child change. Returns the value to push
    /// upward, or `None` if nothing observable changed.
    pub fn apply(&mut self, update: ChildUpdate) -> Option<EventValue> {
        let ChildUpdate { slot, time, value } = update;
        match &self.operation {
            DerivedOperation::Compare {
                op,
                value: constant,
            } => {
                let result = EventValue::BoolValue(op.apply(&value, constant));
                self.record_if_changed(time, result)
            }
            DerivedOperation::Not => {
                let b = value.as_bool()?;
                self.record_if_changed(time, EventValue::BoolValue(!b))
            }
            DerivedOperation::And => self.combine(slot, time, value, |l, r| l && r),
            DerivedOperation::Or => self.combine(slot, time, value, |l, r| l || r),
            DerivedOperation::DurationWhere => {
                let b = value.as_bool()?;
                let current = self.duration_state.map_or(0, |d| d.value_at(time));
                self.duration_state = Some(if b {
                    DurationState::Climbing {
                        base: current,
                        since: time,
                    }
                } else {
                    DurationState::Flat { value: current }
                });
                Some(self.record(time, duration_value(current)))
            }
            DerivedOperation::DurationInCurState => {
                self.duration_state = Some(DurationState::Climbing {
                    base: 0,
                    since: time,
                });
                Some(self.record(time, duration_value(0)))
            }
        }
    }

    /// The state in effect at `t1`. Duration nodes report the running
    /// duration at exactly `t1`; other nodes the state just before `t1`.
    pub fn state_at(&self, t1: u64) -> Option<StatePoint> {
        if let Some(duration) = self.duration_state {
            return Some(StatePoint {
                t1,
                t2: None,
                value: duration_value(duration.value_at(t1)),
            });
        }
        to_state_point(self.result_state.get_state_at(t1), EventValue::clone)
    }

    fn combine(
        &mut self,
        slot: ChildSlot,
        time: u64,
        value: EventValue,
        f: impl Fn(bool, bool) -> bool,
    ) -> Option<EventValue> {
        match slot {
            ChildSlot::Left => self.left_child_state.add_state_dynamic_info(time, value),
            ChildSlot::Right => self.right_child_state.add_state_dynamic_info(time, value),
        }
        let left = bool_after(&self.left_child_state, time)?;
        let right = bool_after(&self.right_child_state, time)?;
        self.record_if_changed(time, EventValue::BoolValue(f(left, right)))
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

/// The boolean state in effect just after `time`, i.e. including a change at `time`.
fn bool_after(state: &StateDynamicsTimeLine<EventValue>, time: u64) -> Option<bool> {
    state
        .get_state_at(time.saturating_add(1))
        .and_then(|p| p.value.as_bool())
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

    fn bool_value(b: bool) -> Option<EventValue> {
        Some(EventValue::BoolValue(b))
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
        assert_eq!(node.apply(state(1, "play")), bool_value(false));
        assert_eq!(node.apply(state(2, "pause")), None);
        assert_eq!(node.apply(state(3, "buffer")), bool_value(true));
    }

    #[test]
    fn not_negates_and_ignores_non_booleans() {
        let mut node = DerivedState::new(DerivedOperation::Not);
        assert_eq!(node.apply(left(1, false)), bool_value(true));
        assert_eq!(
            node.apply(update(ChildSlot::Left, 2, EventValue::IntValue(1))),
            None
        );
    }

    #[test]
    fn and_waits_for_both_children() {
        let mut node = DerivedState::new(DerivedOperation::And);
        assert_eq!(node.apply(left(1, true)), None);
        assert_eq!(node.apply(right(2, true)), bool_value(true));
        assert_eq!(node.apply(left(3, false)), bool_value(false));
        assert_eq!(node.apply(right(4, false)), None);
    }

    #[test]
    fn or_is_true_if_either_child_is() {
        let mut node = DerivedState::new(DerivedOperation::Or);
        node.apply(left(1, false));
        assert_eq!(node.apply(right(1, false)), bool_value(false));
        assert_eq!(node.apply(right(2, true)), bool_value(true));
    }

    #[test]
    fn duration_where_accumulates_while_true() {
        let mut node = DerivedState::new(DerivedOperation::DurationWhere);
        assert_eq!(node.apply(left(100, true)), Some(EventValue::IntValue(0)));
        assert_eq!(int_at(&node, 150), Some(EventValue::IntValue(50)));
        assert_eq!(
            node.apply(left(200, false)),
            Some(EventValue::IntValue(100))
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
    fn empty_node_has_no_state() {
        let node = DerivedState::new(DerivedOperation::And);
        assert_eq!(node.state_at(10), None);
    }
}
