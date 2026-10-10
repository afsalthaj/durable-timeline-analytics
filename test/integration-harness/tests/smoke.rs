// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Smoke tests: the basic end-to-end flow against a running Golem server.
//!
//! Run: `cargo make smoke`

use common_lib::{AgentName, InitializeResult};
use integration_harness::fixtures::{METRIC, column, event, latest, metric, string};
use integration_harness::harness::{self, TestResult};

/// Timeline `latest_event_to_state(playerStateChange)`: a single leaf that is
/// also the root.
#[test]
#[ignore]
fn single_leaf() -> TestResult {
    let client = harness::setup()?;
    let session = harness::unique_session("smoke-leaf");

    let init = client.initialize_timeline(
        &session,
        &metric(METRIC),
        &latest("playerStateChange"),
        None,
    )?;
    let leaf = AgentName::new(format!("{session}-{METRIC}-latest-event-to-state-1"));
    assert_eq!(
        init,
        InitializeResult {
            root_agent: leaf.clone(),
            leaf_agents: vec![leaf.clone()],
        }
    );

    client.add_event(&leaf, &event(100, &[column("playerStateChange", "play")]))?;

    let state = client
        .get_leaf_result(&leaf, 101)?
        .ok_or("no state at t=101")?;
    assert_eq!(state.value, string("play"));
    assert_eq!(state.t1, 100);
    assert_eq!(client.get_leaf_result(&leaf, 99)?, None);
    Ok(())
}

#[test]
#[ignore]
fn state_transitions() -> TestResult {
    let client = harness::setup()?;
    let session = harness::unique_session("smoke-transitions");

    let init = client.initialize_timeline(
        &session,
        &metric(METRIC),
        &latest("playerStateChange"),
        None,
    )?;
    let leaf = init.root_agent;

    for event in [
        event(10, &[column("playerStateChange", "init")]),
        event(20, &[column("playerStateChange", "buffer")]),
        event(30, &[column("playerStateChange", "play")]),
    ] {
        client.add_event(&leaf, &event)?;
    }

    let at = |t| -> Result<_, Box<dyn std::error::Error>> {
        Ok(client.get_leaf_result(&leaf, t)?.map(|point| point.value))
    };
    assert_eq!(at(15)?, Some(string("init")));
    assert_eq!(at(25)?, Some(string("buffer")));
    assert_eq!(at(31)?, Some(string("play")));
    Ok(())
}

/// Redelivering events (at-least-once delivery from Kafka or Pulsar) must not
/// change the timeline.
#[test]
#[ignore]
fn redelivered_events_are_idempotent() -> TestResult {
    let client = harness::setup()?;
    let session = harness::unique_session("smoke-redelivery");
    let init = client.initialize_timeline(
        &session,
        &metric(METRIC),
        &latest("playerStateChange"),
        None,
    )?;
    let leaf = init.root_agent;

    let stream = [
        event(1, &[column("playerStateChange", "init")]),
        event(4, &[column("playerStateChange", "buffer")]),
        event(14, &[column("playerStateChange", "play")]),
    ];
    for event in stream.iter().chain(stream.iter()) {
        client.add_event(&leaf, event)?;
    }

    let last = client
        .get_leaf_result(&leaf, 20)?
        .ok_or("no state at t=20")?;
    assert_eq!(last.t1, 14);
    assert_eq!(last.t2, None);
    let buffering = client.get_leaf_result(&leaf, 5)?.ok_or("no state at t=5")?;
    assert_eq!(buffering.t1, 4);
    assert_eq!(buffering.t2, Some(14));
    Ok(())
}
