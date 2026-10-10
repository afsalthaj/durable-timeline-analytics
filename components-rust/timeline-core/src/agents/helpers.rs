// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use common_lib::{
    AgentName, AggregationConfig, EventValue, StateDynamicsTimeLinePoint, StatePoint,
};

use crate::agents::aggregator::AggregatorClient;
use crate::agents::timeline_processor::TimelineProcessorClient;
use crate::types::*;

pub(crate) fn to_state_point<T>(
    point: Option<&StateDynamicsTimeLinePoint<T>>,
    to_value: impl Fn(&T) -> EventValue,
) -> Option<StatePoint> {
    point.map(|p| StatePoint {
        t1: p.t1,
        t2: p.t2,
        value: to_value(&p.value),
    })
}

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

/// Tracks what the root node last reported to its aggregator.
#[derive(Default)]
pub(crate) struct AggregatorLink {
    pub aggregator: Option<AgentName>,
    pub last_reported: Option<f64>,
}

pub(crate) async fn notify_aggregator(
    aggregation: Option<&AggregationConfig>,
    link: &mut AggregatorLink,
    value: &EventValue,
    group_by_value: Option<&EventValue>,
) {
    let (Some(config), Some(group_value), Some(new_value)) =
        (aggregation, group_by_value, value.as_f64())
    else {
        return;
    };

    let delta = new_value - link.last_reported.unwrap_or(0.0);
    link.last_reported = Some(new_value);

    if delta == 0.0 {
        return;
    }

    let first_call = link.aggregator.is_none();
    let name = link
        .aggregator
        .get_or_insert_with(|| AgentName::for_aggregator(&config.group_by_column, group_value));

    let mut client = AggregatorClient::get(name.0.clone());
    if first_call {
        client.initialize_aggregator(config.functions.clone()).await;
        client.register_session().await;
    }
    client.on_delta(delta).await;
}
