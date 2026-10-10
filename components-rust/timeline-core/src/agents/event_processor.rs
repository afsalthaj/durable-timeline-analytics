// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use golem_rust::{agent_definition, agent_implementation};

use common_lib::{Event, EventColumnName, EventValue, StatePoint, TimelineError};

use crate::agents::helpers::notify_parent;
use crate::compute::LeafState;
use crate::types::{LeafOperation, ParentRef};

/// Leaf node agent. Ingests raw events and computes a leaf timeline operation.
///
/// CIRR example (agent names for session `sess-42`):
///   ```text
///   duration_where(
///     has_existed(playerStateChange == "play")               ← sess-42-cirr-has-existed-4
///     && !has_existed_within(playerStateChange == "seek", 5) ← sess-42-cirr-has-existed-within-6
///     && latest_event_to_state(playerStateChange) == "buffer" ← sess-42-cirr-latest-event-to-state-8
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
/// `"akamai"` is the group-by column value, carried through the cascade. The
/// leaf remembers the last value it saw, so events without the column still
/// carry the session's group.
#[agent_definition]
pub trait EventProcessor {
    fn new(name: String) -> Self;

    /// Tell this leaf what to compute. Re-initializing with the same operation
    /// keeps the existing state.
    fn initialize_leaf(&mut self, operation: LeafOperation);

    /// Wire this leaf to its parent TimelineProcessor so state changes push upward.
    fn set_parent(&mut self, parent: ParentRef);

    /// Tell this leaf which event column to extract for aggregation grouping.
    /// The latest value seen travels up the entire push cascade to the root.
    fn set_group_by_column(&mut self, column: EventColumnName);

    /// Ingest a raw event. Evaluates the leaf operation, and if its state
    /// changed, pushes the new state and the group-by value to the parent.
    async fn add_event(&mut self, event: Event) -> Result<(), TimelineError>;

    /// Point-in-time query. Returns the precomputed state at time `t1`.
    fn get_leaf_result(&self, t1: u64) -> Result<Option<StatePoint>, TimelineError>;
}

struct EventProcessorImpl {
    state: Option<LeafState>,
    parent: Option<ParentRef>,
    group_by_column: Option<EventColumnName>,
    group_by_value: Option<EventValue>,
}

#[agent_implementation]
impl EventProcessor for EventProcessorImpl {
    fn new(_name: String) -> Self {
        Self {
            state: None,
            parent: None,
            group_by_column: None,
            group_by_value: None,
        }
    }

    fn initialize_leaf(&mut self, operation: LeafOperation) {
        if self.state.as_ref().map(LeafState::operation) != Some(&operation) {
            self.state = Some(LeafState::new(operation));
        }
    }

    fn set_parent(&mut self, parent: ParentRef) {
        self.parent = Some(parent);
    }

    fn set_group_by_column(&mut self, column: EventColumnName) {
        self.group_by_column = Some(column);
    }

    async fn add_event(&mut self, event: Event) -> Result<(), TimelineError> {
        let state = self.state.as_mut().ok_or(TimelineError::NotInitialized)?;
        let changes = state.ingest(&event);
        if let Some(value) = self
            .group_by_column
            .as_ref()
            .and_then(|column| event.value_of(column))
        {
            self.group_by_value = Some(value.clone());
        }

        for change in changes {
            notify_parent(
                self.parent.as_ref(),
                change.time,
                change.value,
                self.group_by_value.as_ref(),
            )
            .await;
        }
        Ok(())
    }

    fn get_leaf_result(&self, t1: u64) -> Result<Option<StatePoint>, TimelineError> {
        let state = self.state.as_ref().ok_or(TimelineError::NotInitialized)?;
        Ok(state.state_at(t1))
    }
}
