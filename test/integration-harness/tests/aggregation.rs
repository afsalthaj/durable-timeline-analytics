// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Aggregation tests: cross-session aggregation.
//!
//! Run: `cargo make aggregation`

use common_lib::{
    AgentName, AggregationConfig, AggregationFunction, CompareOp, EventColumnName, TimeLineOp,
};
use integration_harness::fixtures::{column, event, latest, string};
use integration_harness::harness::{self, TestResult};

/// Timeline per session:
/// `latest_event_to_state(playerStateChange) == "buffer" | aggregate(group_by(cdn), count, sum, avg)`
///
/// Agents: equal-to-1 (root), latest-event-to-state-2 (leaf).
#[test]
#[ignore]
fn multi_session_aggregation() -> TestResult {
    let client = harness::setup()?;
    // A CDN value unique to this run keeps the aggregator fresh across reruns.
    let cdn = harness::unique_session("akamai").to_string();
    let timeline = TimeLineOp::compare(
        CompareOp::EqualTo,
        latest("playerStateChange"),
        string("buffer"),
    );
    let aggregation = AggregationConfig {
        group_by_column: EventColumnName::new("cdn"),
        functions: vec![
            AggregationFunction::Count,
            AggregationFunction::Sum,
            AggregationFunction::Avg,
        ],
    };

    for name in ["agg-sess-1", "agg-sess-2"] {
        let session = harness::unique_session(name);
        let init = client.initialize_timeline(&session, &timeline, Some(&aggregation))?;
        for leaf in &init.leaf_agents {
            for (time, state) in [(100, "init"), (200, "buffer")] {
                client.add_event(
                    leaf,
                    &event(
                        time,
                        &[column("playerStateChange", state), column("cdn", &cdn)],
                    ),
                )?;
            }
        }
    }

    let aggregator = AgentName::for_aggregator(&aggregation.group_by_column, &string(&cdn));
    let result = client.get_aggregation_result(&aggregator)?;
    assert_eq!(result.count, 2);
    assert_eq!(result.sum, 2.0);
    assert_eq!(result.avg, 1.0);
    Ok(())
}
