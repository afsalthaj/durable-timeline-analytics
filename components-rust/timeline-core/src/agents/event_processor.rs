// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use golem_rust::{agent_definition, agent_implementation};

use common_lib::{
    Event, EventColumnName, EventPredicate, EventValue, StateDynamicsTimeLine, StatePoint,
    TimelineError,
};

use crate::agents::helpers::*;
use crate::types::*;

/// Leaf node agent. Ingests raw events and computes a leaf timeline operation.
///
/// CIRR example (agent names for session `sess-42`):
///   ```text
///   duration_where(
///     has_existed(playerStateChange == "play")               ← sess-42-has-existed-4
///     && !has_existed_within(playerStateChange == "seek", 5) ← sess-42-has-existed-within-6
///     && latest_event_to_state(playerStateChange) == "buffer" ← sess-42-latest-event-to-state-8
///   ) | aggregate(group_by=cdn, count, sum, avg)
///   ```
///
/// The feeder sends every raw event to all three leaves. Each leaf evaluates its
/// own operation and pushes state changes up to its parent TimelineProcessor.
///
/// Push flow for a single event `{ time: 100, playerStateChange: "play", cdn: "akamai" }`:
///   1. has-existed-4: predicate matches → state becomes `true` → pushes `(true, "akamai")`
///   2. has-existed-within-6: predicate doesn't match → no push
///   3. latest-event-to-state-8: state becomes `"play"` → pushes `("play", "akamai")`
///
/// `"akamai"` is the group-by column value, extracted from the event and carried
/// through the cascade.
#[agent_definition]
pub trait EventProcessor {
    fn new(name: String) -> Self;

    /// Tell this leaf what to compute.
    fn initialize_leaf(&mut self, operation: LeafOperation);

    /// Wire this leaf to its parent TimelineProcessor so state changes push upward.
    fn set_parent(&mut self, parent: ParentRef);

    /// Tell this leaf which event column to extract for aggregation grouping.
    /// The extracted value travels up the entire push cascade to the root.
    fn set_group_by_column(&mut self, column: EventColumnName);

    /// Ingest a raw event. Evaluates the leaf operation, and if its state
    /// changed, pushes the new state and the group-by value to the parent.
    async fn add_event(&mut self, event: Event) -> Result<(), TimelineError>;

    /// Point-in-time query. Returns the precomputed state at time `t1`.
    fn get_leaf_result(&self, t1: u64) -> Result<Option<StatePoint>, TimelineError>;
}

struct EventProcessorImpl {
    operation: Option<LeafOperation>,
    parent: Option<ParentRef>,
    group_by_column: Option<EventColumnName>,
    latest_event_state: StateDynamicsTimeLine<EventValue>,
    has_existed_state: StateDynamicsTimeLine<bool>,
}

#[agent_implementation]
impl EventProcessor for EventProcessorImpl {
    fn new(_name: String) -> Self {
        Self {
            operation: None,
            parent: None,
            group_by_column: None,
            latest_event_state: StateDynamicsTimeLine::default(),
            has_existed_state: StateDynamicsTimeLine::default(),
        }
    }

    fn initialize_leaf(&mut self, operation: LeafOperation) {
        self.operation = Some(operation);
    }

    fn set_parent(&mut self, parent: ParentRef) {
        self.parent = Some(parent);
    }

    fn set_group_by_column(&mut self, column: EventColumnName) {
        self.group_by_column = Some(column);
    }

    async fn add_event(&mut self, event: Event) -> Result<(), TimelineError> {
        let operation = self
            .operation
            .clone()
            .ok_or(TimelineError::NotInitialized)?;
        let group_by_value = self
            .group_by_column
            .as_ref()
            .and_then(|column| event.value_of(column))
            .cloned();

        let changes = match &operation {
            LeafOperation::LatestEventToState(column) => self.track_latest(column, &event),
            LeafOperation::HasExisted(predicate) => self.track_has_existed(predicate, &event, None),
            LeafOperation::HasExistedWithin { predicate, within } => {
                self.track_has_existed(predicate, &event, Some(*within))
            }
        };

        for change in changes {
            notify_parent(
                self.parent.as_ref(),
                change.time,
                change.value,
                group_by_value.as_ref(),
            )
            .await;
        }
        Ok(())
    }

    fn get_leaf_result(&self, t1: u64) -> Result<Option<StatePoint>, TimelineError> {
        match self
            .operation
            .as_ref()
            .ok_or(TimelineError::NotInitialized)?
        {
            LeafOperation::LatestEventToState(_) => Ok(to_state_point(
                self.latest_event_state.state_holding_at(t1),
                EventValue::clone,
            )),
            LeafOperation::HasExisted(_) | LeafOperation::HasExistedWithin { .. } => Ok(
                to_state_point(self.has_existed_state.state_holding_at(t1), |b| {
                    EventValue::BoolValue(*b)
                }),
            ),
        }
    }
}

/// A state transition a leaf reports to its parent.
struct LeafChange {
    time: u64,
    value: EventValue,
}

impl EventProcessorImpl {
    fn track_latest(&mut self, column: &EventColumnName, event: &Event) -> Vec<LeafChange> {
        let Some(value) = event.value_of(column) else {
            return vec![];
        };
        let previous = self
            .latest_event_state
            .get_state_at(event.time)
            .map(|p| p.value.clone());
        self.latest_event_state
            .add_state_dynamic_info(event.time, value.clone());
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
        &mut self,
        predicate: &EventPredicate,
        event: &Event,
        within: Option<u64>,
    ) -> Vec<LeafChange> {
        let state = &mut self.has_existed_state;
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
                state.add_state_dynamic_info(time + within, false);
                changes.push(LeafChange {
                    time: time + within,
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
}
