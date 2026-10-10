// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Standalone Kafka consumer/feeder binary.
//!
//! Consumes events from Kafka and feeds them to Golem agents.
//!
//! # Environment variables
//! - `KAFKA_BROKER`: Kafka broker address (default: "localhost:9092")
//! - `KAFKA_TOPIC`: Kafka topic name (default: "timeline-events")
//! - `KAFKA_GROUP`: Consumer group ID (default: "timeline-feeder")
//! - `GOLEM_APP_DIR`: Directory containing the app's `golem.yaml` (default: ".")
//! - `LEAF_AGENTS`: Comma-separated leaf agent names (required)
//! - `GOLEM_CLI`: Path to the `golem` binary (default: `golem` on `PATH`)

use std::env;
use std::error::Error;
use std::time::Duration;

use common_lib::AgentName;
use kafka_consumer_feeder::{FeederConfig, GolemFeeder};

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let leaf_agents: Vec<AgentName> = env::var("LEAF_AGENTS")
        .map_err(|_| "LEAF_AGENTS must be set")?
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(AgentName::new)
        .collect();

    let config = FeederConfig {
        kafka_broker: env_or("KAFKA_BROKER", "localhost:9092"),
        kafka_topic: env_or("KAFKA_TOPIC", "timeline-events"),
        consumer_group: env_or("KAFKA_GROUP", "timeline-feeder"),
        golem_app_dir: env_or("GOLEM_APP_DIR", ".").into(),
        leaf_agents,
        idle_timeout: Duration::from_secs(5),
    };

    log::info!(
        "Starting Kafka consumer/feeder: broker={}, topic={}, group={}, agents={:?}",
        config.kafka_broker,
        config.kafka_topic,
        config.consumer_group,
        config.leaf_agents,
    );

    let start = std::time::Instant::now();
    let feeder = GolemFeeder::new(config)?;
    let count = feeder.run_until_idle().await?;
    println!(
        "Fed {count} events to Golem agents in {:.2?}",
        start.elapsed()
    );
    Ok(())
}
