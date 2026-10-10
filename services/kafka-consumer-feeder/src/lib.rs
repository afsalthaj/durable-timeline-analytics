// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Kafka consumer that feeds events to the timeline's leaf agents.
//!
//! Consumes JSON-encoded `common_lib::Event`s from a Kafka topic and sends each
//! one to every leaf EventProcessor through the `golem` CLI. Reusable as a
//! library (for the test harness) or as a standalone binary.
//!
//! # As a library
//! ```rust,ignore
//! let feeder = GolemFeeder::new(feeder_config)?;
//! let fed = feeder.run_until_idle().await?;
//! ```

use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;

use common_lib::{AgentName, Event};
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::{ClientConfig, Message};
use timeline_client::{GolemCli, TimelineClient};

/// Configuration for the Golem feeder.
pub struct FeederConfig {
    /// Kafka broker address (e.g. "localhost:9092").
    pub kafka_broker: String,
    /// Kafka topic to consume from.
    pub kafka_topic: String,
    /// Kafka consumer group ID.
    pub consumer_group: String,
    /// Directory of the Golem application (containing `golem.yaml`).
    pub golem_app_dir: PathBuf,
    /// Leaf agents to feed events to, as returned by `initialize_timeline`.
    pub leaf_agents: Vec<AgentName>,
    /// Stop after this long without a message.
    pub idle_timeout: Duration,
}

/// Consumes from Kafka and feeds events to Golem agents.
pub struct GolemFeeder {
    consumer: StreamConsumer,
    client: TimelineClient,
    leaf_agents: Vec<AgentName>,
    idle_timeout: Duration,
}

impl GolemFeeder {
    /// Create a feeder subscribed to the configured topic.
    pub fn new(config: FeederConfig) -> Result<Self, Box<dyn Error>> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &config.kafka_broker)
            .set("group.id", &config.consumer_group)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "true")
            .create()?;
        consumer.subscribe(&[&config.kafka_topic])?;

        Ok(Self {
            consumer,
            client: TimelineClient::new(GolemCli::new(config.golem_app_dir)),
            leaf_agents: config.leaf_agents,
            idle_timeout: config.idle_timeout,
        })
    }

    /// Consume events until none arrive for `idle_timeout`, feeding each one to
    /// every leaf agent. Returns the number of events fed.
    pub async fn run_until_idle(&self) -> Result<u64, Box<dyn Error>> {
        let mut count: u64 = 0;
        loop {
            let message = match tokio::time::timeout(self.idle_timeout, self.consumer.recv()).await
            {
                Ok(Ok(message)) => message,
                Ok(Err(e)) => {
                    log::error!("Kafka consumer error: {e}");
                    continue;
                }
                Err(_) => {
                    log::info!("No messages for {:?}, considering idle", self.idle_timeout);
                    break;
                }
            };

            let event = match message.payload_view::<str>() {
                Some(Ok(payload)) => match serde_json::from_str::<Event>(payload) {
                    Ok(event) => event,
                    Err(e) => {
                        log::error!("Skipping malformed event: {e}");
                        continue;
                    }
                },
                Some(Err(e)) => {
                    log::error!("Skipping non-UTF-8 payload: {e}");
                    continue;
                }
                None => {
                    log::warn!("Skipping empty payload");
                    continue;
                }
            };

            self.feed(event).await?;
            count += 1;
            log::info!("Processed event #{count}");
        }
        Ok(count)
    }

    async fn feed(&self, event: Event) -> Result<(), Box<dyn Error>> {
        let client = self.client.clone();
        let leaves = self.leaf_agents.clone();
        tokio::task::spawn_blocking(move || {
            for leaf in &leaves {
                if let Err(e) = client.add_event(leaf, &event) {
                    log::error!("Failed to feed event to {leaf}: {e}");
                }
            }
        })
        .await?;
        Ok(())
    }
}
