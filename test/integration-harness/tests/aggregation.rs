// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Aggregation tests: cross-session aggregation.
//!
//! Run: `cargo make aggregation`

use common_lib::{
    AgentName, AggregationConfig, AggregationFunction, AggregationResult, CompareOp, Event,
    EventColumnName, MetricName, TimeLineOp,
};
use integration_harness::fixtures::{column, event, latest, string};
use integration_harness::harness::{self, TestResult};
use timeline_client::TimelineClient;

fn by_cdn() -> AggregationConfig {
    AggregationConfig {
        group_by_column: EventColumnName::new("cdn"),
        functions: vec![
            AggregationFunction::Count,
            AggregationFunction::Sum,
            AggregationFunction::Avg,
        ],
    }
}

/// A metric name unique to this run, so its aggregators start empty.
fn unique_metric(prefix: &str) -> MetricName {
    MetricName::new(harness::unique_session(prefix).as_str())
}

fn aggregator(metric: &MetricName, cdn: &str) -> AgentName {
    AgentName::for_aggregator(metric, &EventColumnName::new("cdn"), &string(cdn))
}

fn player(time: u64, state: &str, cdn: &str) -> Event {
    event(
        time,
        &[column("playerStateChange", state), column("cdn", cdn)],
    )
}

/// Initializes `metric` for a fresh session and feeds it `events`.
fn run_session(
    client: &TimelineClient,
    metric: &MetricName,
    timeline: &TimeLineOp,
    events: &[Event],
) -> TestResult {
    let session = harness::unique_session("agg-sess");
    let init = client.initialize_timeline(&session, metric, timeline, Some(&by_cdn()))?;
    for event in events {
        for leaf in &init.leaf_agents {
            client.add_event(leaf, event)?;
        }
    }
    Ok(())
}

fn is_buffering() -> TimeLineOp {
    TimeLineOp::compare(
        CompareOp::EqualTo,
        latest("playerStateChange"),
        string("buffer"),
    )
}

/// `latest_event_to_state(playerStateChange) == "buffer" | aggregate(group_by(cdn), ...)`.
/// A session that never buffers is still counted, with value 0.
#[test]
#[ignore]
fn sessions_without_buffering_are_counted() -> TestResult {
    let client = harness::setup()?;
    let metric = unique_metric("agg-count");
    let timeline = is_buffering();

    for _ in 0..2 {
        run_session(
            &client,
            &metric,
            &timeline,
            &[
                player(100, "init", "akamai"),
                player(200, "buffer", "akamai"),
            ],
        )?;
    }
    run_session(
        &client,
        &metric,
        &timeline,
        &[player(100, "init", "akamai"), player(200, "play", "akamai")],
    )?;

    let result = client.get_aggregation_result(&aggregator(&metric, "akamai"))?;
    assert_eq!(result.count, 3);
    assert_eq!(result.sum, 2.0);
    assert_eq!(result.min, Some(0.0));
    assert_eq!(result.max, Some(1.0));
    Ok(())
}

/// `duration_where(latest_event_to_state(playerStateChange) == "buffer")`:
/// a session that is still buffering counts up to the queried time.
#[test]
#[ignore]
fn ongoing_buffering_counts_up_to_query_time() -> TestResult {
    let client = harness::setup()?;
    let metric = unique_metric("agg-rising");
    let timeline = TimeLineOp::duration_where(is_buffering());

    // Still buffering since 100.
    run_session(
        &client,
        &metric,
        &timeline,
        &[player(100, "buffer", "akamai")],
    )?;
    // Buffered from 100 to 150.
    run_session(
        &client,
        &metric,
        &timeline,
        &[
            player(100, "buffer", "akamai"),
            player(150, "play", "akamai"),
        ],
    )?;

    let akamai = aggregator(&metric, "akamai");
    assert_eq!(
        client.get_aggregation_result_at(&akamai, 200)?,
        AggregationResult {
            count: 2,
            sum: 150.0,
            avg: 75.0,
            min: Some(50.0),
            max: Some(100.0),
        }
    );
    // Without a time, the latest reported event time (150) is used.
    assert_eq!(client.get_aggregation_result(&akamai)?.sum, 100.0);
    Ok(())
}

/// Two metrics on the same session get separate agents and aggregators.
#[test]
#[ignore]
fn metrics_on_one_session_are_independent() -> TestResult {
    let client = harness::setup()?;
    let session = harness::unique_session("agg-two-metrics");
    let buffering = unique_metric("agg-buffering");
    let playing = unique_metric("agg-playing");
    let is_playing = TimeLineOp::compare(
        CompareOp::EqualTo,
        latest("playerStateChange"),
        string("play"),
    );

    let first =
        client.initialize_timeline(&session, &buffering, &is_buffering(), Some(&by_cdn()))?;
    let second = client.initialize_timeline(&session, &playing, &is_playing, Some(&by_cdn()))?;
    assert_ne!(first.root_agent, second.root_agent);

    let leaves = first.leaf_agents.iter().chain(second.leaf_agents.iter());
    for leaf in leaves {
        client.add_event(leaf, &player(100, "buffer", "akamai"))?;
    }

    let buffering_result = client.get_aggregation_result(&aggregator(&buffering, "akamai"))?;
    let playing_result = client.get_aggregation_result(&aggregator(&playing, "akamai"))?;
    assert_eq!(buffering_result.count, 1);
    assert_eq!(buffering_result.sum, 1.0);
    assert_eq!(playing_result.count, 1);
    assert_eq!(playing_result.sum, 0.0);
    Ok(())
}

/// A session whose group value changes moves to the new group's aggregator.
#[test]
#[ignore]
fn session_moves_when_its_group_changes() -> TestResult {
    let client = harness::setup()?;
    let metric = unique_metric("agg-move");

    run_session(
        &client,
        &metric,
        &is_buffering(),
        &[
            player(100, "init", "akamai"),
            player(200, "buffer", "fastly"),
        ],
    )?;

    assert_eq!(
        client
            .get_aggregation_result(&aggregator(&metric, "akamai"))?
            .count,
        0
    );
    let fastly = client.get_aggregation_result(&aggregator(&metric, "fastly"))?;
    assert_eq!(fastly.count, 1);
    assert_eq!(fastly.sum, 1.0);
    Ok(())
}
