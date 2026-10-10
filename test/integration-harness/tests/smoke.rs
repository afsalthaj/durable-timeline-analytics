// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Smoke tests: the basic end-to-end flow against a running Golem server.
//!
//! Run: `cargo make smoke`

use common_lib::{AgentName, InitializeResult};
use integration_harness::fixtures::{column, event, latest, string};
use integration_harness::harness::{self, TestResult};

/// Timeline `latest_event_to_state(playerStateChange)`: a single leaf that is
/// also the root.
#[test]
#[ignore]
fn single_leaf() -> TestResult {
    let client = harness::setup()?;
    let session = harness::unique_session("smoke-leaf");

    let init = client.initialize_timeline(&session, &latest("playerStateChange"), None)?;
    let leaf = AgentName::new(format!("{session}-latest-event-to-state-1"));
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

    let init = client.initialize_timeline(&session, &latest("playerStateChange"), None)?;
    let leaf = init.root_agent;

    for (time, state) in [(10, "init"), (20, "buffer"), (30, "play")] {
        client.add_event(&leaf, &event(time, &[column("playerStateChange", state)]))?;
    }

    let at = |t| -> Result<_, Box<dyn std::error::Error>> {
        Ok(client.get_leaf_result(&leaf, t)?.map(|point| point.value))
    };
    assert_eq!(at(15)?, Some(string("init")));
    assert_eq!(at(25)?, Some(string("buffer")));
    assert_eq!(at(31)?, Some(string("play")));
    Ok(())
}
