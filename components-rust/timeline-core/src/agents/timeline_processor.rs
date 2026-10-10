// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use golem_rust::{agent_definition, agent_implementation};

use common_lib::{AggregationConfig, StatePoint, TimelineError};

use crate::agents::helpers::{AggregatorLink, notify_aggregator, notify_parent};
use crate::compute::{ChildUpdate, DerivedState};
use crate::types::{ChildStateChange, DerivedOperation, ParentRef};

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

    /// Tell this node what to compute. Re-initializing with the same operation
    /// keeps the existing state.
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

struct TimelineProcessorImpl {
    state: Option<DerivedState>,
    parent: Option<ParentRef>,
    aggregation: Option<AggregationConfig>,
    aggregator_link: AggregatorLink,
}

#[agent_implementation]
impl TimelineProcessor for TimelineProcessorImpl {
    fn new(_name: String) -> Self {
        Self {
            state: None,
            parent: None,
            aggregation: None,
            aggregator_link: AggregatorLink::default(),
        }
    }

    fn initialize_derived(&mut self, operation: DerivedOperation) {
        if self.state.as_ref().map(DerivedState::operation) != Some(&operation) {
            self.state = Some(DerivedState::new(operation));
        }
    }

    fn set_parent(&mut self, parent: ParentRef) {
        self.parent = Some(parent);
    }

    fn set_aggregation(&mut self, config: AggregationConfig) {
        self.aggregation = Some(config);
    }

    async fn on_child_state_changed(&mut self, change: ChildStateChange) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let time = change.time;
        let Some(output) = state.apply(ChildUpdate {
            slot: change.slot,
            time,
            value: change.value,
        }) else {
            return;
        };

        let group_by_value = change.group_by_value.as_ref();
        notify_parent(self.parent.as_ref(), time, output.clone(), group_by_value).await;
        notify_aggregator(
            self.aggregation.as_ref(),
            &mut self.aggregator_link,
            &output,
            group_by_value,
        )
        .await;
    }

    fn get_derived_result(&self, t1: u64) -> Result<Option<StatePoint>, TimelineError> {
        let state = self.state.as_ref().ok_or(TimelineError::NotInitialized)?;
        Ok(state.state_at(t1))
    }
}
