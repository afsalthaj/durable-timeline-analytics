// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use golem_rust::{agent_definition, agent_implementation};

use common_lib::{AggregationConfig, EventValue, StateDynamicsTimeLine, StatePoint, TimelineError};

use crate::agents::helpers::*;
use crate::types::*;

/// Derived node agent. Receives state changes from children, recomputes its own
/// state, and pushes upward. The root node (no parent) pushes to the Aggregator.
///
/// CIRR example — the derived nodes for session `sess-42`:
///   ```text
///   sess-42-duration-where-1   DurationWhere(and-2)          ← root
///   sess-42-and-2              And(and-3, equal-to-7)
///   sess-42-and-3              And(has-existed-4, not-5)
///   sess-42-not-5              Not(has-existed-within-6)
///   sess-42-equal-to-7         EqualTo(latest-event-to-state-8, "buffer")
///   ```
///
/// Push cascade when the user starts buffering at time 200 (cdn = "akamai"):
///   1. latest-event-to-state-8 pushes "buffer" → equal-to-7 computes `true`
///   2. equal-to-7 pushes `true` → and-2 → both inputs true → `true`
///   3. and-2 pushes `true` → duration-where-1 starts counting
///   4. duration-where-1 is the root with aggregation → reports to `aggregator-cdn-akamai`
#[agent_definition]
pub trait TimelineProcessor {
    fn new(name: String) -> Self;

    /// Tell this node what to compute.
    fn initialize_derived(&mut self, operation: DerivedOperation);

    /// Wire this node to its parent. Not set on the root — the root pushes to the Aggregator.
    fn set_parent(&mut self, parent: ParentRef);

    /// Called only on the root node. Tells it which event column determines
    /// aggregator routing and which functions to compute. The aggregator is
    /// created lazily on the first non-zero delta.
    fn set_aggregation(&mut self, config: AggregationConfig);

    /// Receive a state change from a child. `group_by_value` is the aggregation
    /// column value (e.g. "akamai") extracted by the leaf and propagated through
    /// every node in the cascade.
    async fn on_child_state_changed(&mut self, change: ChildStateChange);

    /// Point-in-time query on precomputed state.
    fn get_derived_result(&self, t1: u64) -> Result<Option<StatePoint>, TimelineError>;
}

/// Running state of a duration node.
#[derive(Clone, Copy, Debug)]
enum DurationState {
    /// Counting up since `since`, starting from `base`.
    Climbing { base: u64, since: u64 },
    /// Holding at `value`.
    Flat { value: u64 },
}

impl DurationState {
    fn value_at(self, t: u64) -> u64 {
        match self {
            DurationState::Climbing { base, since } => base + t.saturating_sub(since),
            DurationState::Flat { value } => value,
        }
    }
}

struct TimelineProcessorImpl {
    operation: Option<DerivedOperation>,
    parent: Option<ParentRef>,
    aggregation: Option<AggregationConfig>,
    aggregator_link: AggregatorLink,
    left_child_state: StateDynamicsTimeLine<EventValue>,
    right_child_state: StateDynamicsTimeLine<EventValue>,
    result_state: StateDynamicsTimeLine<EventValue>,
    duration_state: Option<DurationState>,
}

#[agent_implementation]
impl TimelineProcessor for TimelineProcessorImpl {
    fn new(_name: String) -> Self {
        Self {
            operation: None,
            parent: None,
            aggregation: None,
            aggregator_link: AggregatorLink::default(),
            left_child_state: StateDynamicsTimeLine::default(),
            right_child_state: StateDynamicsTimeLine::default(),
            result_state: StateDynamicsTimeLine::default(),
            duration_state: None,
        }
    }

    fn initialize_derived(&mut self, operation: DerivedOperation) {
        self.operation = Some(operation);
    }

    fn set_parent(&mut self, parent: ParentRef) {
        self.parent = Some(parent);
    }

    fn set_aggregation(&mut self, config: AggregationConfig) {
        self.aggregation = Some(config);
    }

    async fn on_child_state_changed(&mut self, change: ChildStateChange) {
        let Some(operation) = self.operation.clone() else {
            return;
        };
        let time = change.time;
        let Some(output) = self.recompute(&operation, change.slot, time, change.value) else {
            return;
        };

        notify_parent(
            self.parent.as_ref(),
            time,
            output.clone(),
            change.group_by_value.as_ref(),
        )
        .await;
        notify_aggregator(
            self.aggregation.as_ref(),
            &mut self.aggregator_link,
            &output,
            change.group_by_value.as_ref(),
        )
        .await;
    }

    fn get_derived_result(&self, t1: u64) -> Result<Option<StatePoint>, TimelineError> {
        if self.operation.is_none() {
            return Err(TimelineError::NotInitialized);
        }
        if let Some(duration) = self.duration_state {
            return Ok(Some(StatePoint {
                t1,
                t2: None,
                value: duration_value(duration.value_at(t1)),
            }));
        }
        Ok(to_state_point(
            self.result_state.state_holding_at(t1),
            EventValue::clone,
        ))
    }
}

fn duration_value(duration: u64) -> EventValue {
    EventValue::IntValue(i64::try_from(duration).unwrap_or(i64::MAX))
}

impl TimelineProcessorImpl {
    /// Updates this node's state for a child change. Returns the value to push
    /// upward, or `None` if nothing observable changed.
    fn recompute(
        &mut self,
        operation: &DerivedOperation,
        slot: ChildSlot,
        time: u64,
        value: EventValue,
    ) -> Option<EventValue> {
        match operation {
            DerivedOperation::Compare {
                op,
                value: constant,
            } => self.record_if_changed(time, EventValue::BoolValue(op.apply(&value, constant))),
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

/// The boolean state in effect just after `time`, i.e. including a change at `time`.
fn bool_after(state: &StateDynamicsTimeLine<EventValue>, time: u64) -> Option<bool> {
    state.get_state_at(time + 1).and_then(|p| p.value.as_bool())
}
