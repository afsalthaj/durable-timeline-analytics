// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Kafka pipeline test: event-generator → kafka-producer → Kafka →
//! kafka-consumer-feeder → Golem → assert.
//!
//! Run: `cargo make kafka-test`

use std::time::Duration;

use event_generator::last_player_state;
use integration_harness::fixtures::latest;
use integration_harness::harness::{self, TestResult};
use kafka_consumer_feeder::{FeederConfig, GolemFeeder};
use kafka_producer::TimelineKafkaProducer;

const KAFKA_BROKER: &str = "localhost:9092";

/// Timeline `latest_event_to_state(playerStateChange)`: after the pipeline has
/// run, the leaf holds the last generated player state.
#[test]
#[ignore]
fn kafka_event_pipeline() -> TestResult {
    let client = harness::setup()?;
    harness::ensure_kafka_running()?;

    let session = harness::unique_session("kafka-pipe");
    let topic = format!("timeline-{session}");
    let init = client.initialize_timeline(&session, &latest("playerStateChange"), None)?;

    let events = event_generator::generate_cirr_session_events_n(session.as_str(), "akamai", 10);
    let last = last_player_state(&events).ok_or("no playerStateChange event generated")?;

    let runtime = tokio::runtime::Runtime::new()?;
    let fed = runtime.block_on(async {
        let producer = TimelineKafkaProducer::new(KAFKA_BROKER, &topic)?;
        producer
            .publish_session_events(session.as_str(), &events)
            .await?;

        let feeder = GolemFeeder::new(FeederConfig {
            kafka_broker: KAFKA_BROKER.to_string(),
            kafka_topic: topic.clone(),
            consumer_group: format!("{topic}-feeder"),
            golem_app_dir: harness::workspace_root(),
            leaf_agents: init.leaf_agents.clone(),
            idle_timeout: Duration::from_secs(10),
        })?;
        feeder.run_until_idle().await
    })?;
    assert_eq!(fed, u64::try_from(events.len())?);

    let state = client
        .get_leaf_result(&init.root_agent, last.time + 1)?
        .ok_or("leaf has no state after the pipeline ran")?;
    assert_eq!(state.value, last.value);
    Ok(())
}
