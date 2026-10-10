// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use common_lib::{AgentName, EventValue};

use crate::agents::aggregator::AggregatorClient;
use crate::agents::timeline_processor::TimelineProcessorClient;
use crate::compute::AggregatorLink;
use crate::types::{AggregationTarget, ChildStateChange, ParentRef, SessionReport, SessionValue};

pub(crate) async fn notify_parent(
    parent: Option<&ParentRef>,
    time: u64,
    value: EventValue,
    group_by_value: Option<&EventValue>,
) {
    if let Some(parent) = parent {
        let mut client = TimelineProcessorClient::get(parent.agent_name.0.clone());
        client
            .on_child_state_changed(ChildStateChange {
                slot: parent.slot,
                time,
                value,
                group_by_value: group_by_value.cloned(),
            })
            .await;
    }
}

/// What a root node reports to its aggregator after a change at `time`.
pub(crate) struct RootUpdate<'a> {
    pub session: &'a AgentName,
    pub time: u64,
    pub value: SessionValue,
    pub group_by_value: &'a EventValue,
}

/// Reports a root node's value to the aggregator of its current group, moving
/// the session out of its previous group's aggregator if the group changed.
pub(crate) async fn notify_aggregator(
    target: &AggregationTarget,
    link: &mut AggregatorLink,
    update: RootUpdate<'_>,
) {
    let aggregator = AgentName::for_aggregator(
        &target.metric,
        &target.config.group_by_column,
        update.group_by_value,
    );
    let Some(route) = link.route(aggregator, update.value) else {
        return;
    };
    if let Some(previous) = route.remove_from {
        AggregatorClient::get(previous.0)
            .remove_session(update.session.clone())
            .await;
    }
    AggregatorClient::get(route.report_to.0)
        .report(SessionReport {
            session: update.session.clone(),
            time: update.time,
            value: update.value,
        })
        .await;
}
