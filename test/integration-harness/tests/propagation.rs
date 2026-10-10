// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Propagation tests: the push cascade from leaves to the root.
//!
//! Run: `cargo make propagation`

use common_lib::{AgentName, Event, EventValue, SessionId, TimeLineOp};
use integration_harness::fixtures::{cirr, column, event, has_existed};
use integration_harness::harness::{self, TestResult};
use timeline_client::TimelineClient;

fn node(session: &SessionId, suffix: &str) -> AgentName {
    AgentName::new(format!("{session}-{suffix}"))
}

fn send(client: &TimelineClient, leaves: &[AgentName], event: &Event) -> TestResult {
    for leaf in leaves {
        client.add_event(leaf, event)?;
    }
    Ok(())
}

fn root_value(
    client: &TimelineClient,
    root: &AgentName,
    t: u64,
) -> Result<Option<EventValue>, Box<dyn std::error::Error>> {
    Ok(client.get_derived_result(root, t)?.map(|point| point.value))
}

/// Timeline: `has_existed(status == "active") && !has_existed(status == "error")`
///
/// Agents (pre-order): and-1, has-existed-2, not-3, has-existed-4.
#[test]
#[ignore]
fn boolean_logic() -> TestResult {
    let client = harness::setup()?;
    let session = harness::unique_session("prop-bool");
    let timeline = TimeLineOp::and(
        has_existed("status", "active"),
        !has_existed("status", "error"),
    );

    let init = client.initialize_timeline(&session, &timeline, None)?;
    let root = node(&session, "and-1");
    assert_eq!(init.root_agent, root);
    assert_eq!(
        init.leaf_agents,
        vec![
            node(&session, "has-existed-2"),
            node(&session, "has-existed-4")
        ]
    );

    // active: has_existed(active) = true, has_existed(error) = false → true
    send(
        &client,
        &init.leaf_agents,
        &event(10, &[column("status", "active")]),
    )?;
    assert_eq!(
        root_value(&client, &root, 11)?,
        Some(EventValue::BoolValue(true))
    );

    // error: has_existed(error) = true → !true = false → false
    send(
        &client,
        &init.leaf_agents,
        &event(20, &[column("status", "error")]),
    )?;
    assert_eq!(
        root_value(&client, &root, 21)?,
        Some(EventValue::BoolValue(false))
    );

    // The past is still queryable.
    assert_eq!(
        root_value(&client, &root, 15)?,
        Some(EventValue::BoolValue(true))
    );
    Ok(())
}

/// The CIRR expression. Buffering after play, with no recent seek, counts.
///
/// Agents (pre-order): duration-where-1, and-2, and-3, has-existed-4, not-5,
/// has-existed-within-6, equal-to-7, latest-event-to-state-8.
#[test]
#[ignore]
fn cirr_duration() -> TestResult {
    let client = harness::setup()?;
    let session = harness::unique_session("prop-cirr");

    let init = client.initialize_timeline(&session, &cirr(), None)?;
    let root = node(&session, "duration-where-1");
    assert_eq!(init.root_agent, root);
    assert_eq!(
        init.leaf_agents,
        vec![
            node(&session, "has-existed-4"),
            node(&session, "has-existed-within-6"),
            node(&session, "latest-event-to-state-8"),
        ]
    );

    let player = |time, state| event(time, &[column("playerStateChange", state)]);

    send(&client, &init.leaf_agents, &player(100, "play"))?;
    assert_eq!(
        root_value(&client, &root, 150)?,
        Some(EventValue::IntValue(0))
    );

    // Buffering from 200: the duration climbs.
    send(&client, &init.leaf_agents, &player(200, "buffer"))?;
    assert_eq!(
        root_value(&client, &root, 250)?,
        Some(EventValue::IntValue(50))
    );

    // Playing again at 300: the duration holds at 100.
    send(&client, &init.leaf_agents, &player(300, "play"))?;
    assert_eq!(
        root_value(&client, &root, 400)?,
        Some(EventValue::IntValue(100))
    );
    Ok(())
}
