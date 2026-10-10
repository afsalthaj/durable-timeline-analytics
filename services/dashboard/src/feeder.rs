// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Spawns one Kafka consumer task per registered metric. Each task initializes
//! the metric's timeline for every new session it sees and routes events to the
//! leaves that read the event's columns.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common_lib::{AgentName, AggregationConfig, Event, EventColumnName, SessionId, TimeLineOp};
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::{ClientConfig, Message};
use timeline_client::TimelineClient;

use crate::registry::{AgentSuffix, MetricName, Registration};

/// Tracks which metrics have active feeder tasks.
#[derive(Clone)]
pub struct FeederManager {
    kafka_broker: String,
    kafka_topic: String,
    client: TimelineClient,
    active_metrics: Arc<Mutex<HashSet<MetricName>>>,
}

/// Event column → suffixes of the leaves that read it.
type Routes = HashMap<EventColumnName, Vec<AgentSuffix>>;

/// Everything one feeder task needs.
struct MetricFeed {
    metric: MetricName,
    timeline: TimeLineOp,
    aggregation: Option<AggregationConfig>,
    routes: Routes,
}

impl FeederManager {
    pub fn new(kafka_broker: String, kafka_topic: String, client: TimelineClient) -> Self {
        Self {
            kafka_broker,
            kafka_topic,
            client,
            active_metrics: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Start a background feeder task for the metric, unless one is running.
    pub fn start_feeder(&self, registration: &Registration) {
        let metric = registration.metric.name.clone();
        match self.active_metrics.lock() {
            Ok(mut active) => {
                if !active.insert(metric.clone()) {
                    return;
                }
            }
            Err(_) => return,
        }

        let feed = MetricFeed {
            metric,
            timeline: registration.timeline.clone(),
            aggregation: registration.aggregation.clone(),
            routes: registration.metric.routing_table.routes.clone(),
        };
        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(e) = manager.run(&feed).await {
                eprintln!("[feeder:{}] error: {e}", feed.metric.0);
            }
        });
    }

    async fn run(&self, feed: &MetricFeed) -> Result<(), String> {
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let group_id = format!("dashboard-feeder-{}-{started}", feed.metric.0);

        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &self.kafka_broker)
            .set("group.id", &group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "true")
            .create()
            .map_err(|e| e.to_string())?;
        consumer
            .subscribe(&[&self.kafka_topic])
            .map_err(|e| e.to_string())?;
        println!("[feeder:{}] started, group={group_id}", feed.metric.0);

        let mut initialized: HashSet<SessionId> = HashSet::new();
        let mut event_count: u64 = 0;
        loop {
            let message = match tokio::time::timeout(Duration::from_secs(10), consumer.recv()).await
            {
                Ok(Ok(message)) => message,
                Ok(Err(e)) => {
                    eprintln!("[feeder:{}] kafka error: {e}", feed.metric.0);
                    continue;
                }
                Err(_) => {
                    println!("[feeder:{}] idle (no messages for 10s)", feed.metric.0);
                    continue;
                }
            };

            let Some(Ok(key)) = message.key_view::<str>() else {
                continue;
            };
            let session = SessionId::new(key);
            let Some(Ok(payload)) = message.payload_view::<str>() else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<Event>(payload) else {
                continue;
            };

            if !initialized.contains(&session) {
                match self.initialize(feed, &session).await {
                    Ok(()) => {
                        println!("[feeder:{}] initialized session {session}", feed.metric.0);
                        initialized.insert(session.clone());
                    }
                    Err(e) => {
                        eprintln!(
                            "[feeder:{}] failed to initialize session {session}: {e}",
                            feed.metric.0
                        );
                        continue;
                    }
                }
            }

            for leaf in leaves_for(&feed.routes, &session, &event) {
                if let Err(e) = self.add_event(&leaf, &event).await {
                    eprintln!("[feeder:{}] failed to send to {leaf}: {e}", feed.metric.0);
                }
            }

            event_count += 1;
            if event_count.is_multiple_of(10) {
                println!("[feeder:{}] processed {event_count} events", feed.metric.0);
            }
        }
    }

    async fn initialize(&self, feed: &MetricFeed, session: &SessionId) -> Result<(), String> {
        let client = self.client.clone();
        let session = session.clone();
        let timeline = feed.timeline.clone();
        let aggregation = feed.aggregation.clone();
        tokio::task::spawn_blocking(move || {
            client
                .initialize_timeline(&session, &timeline, aggregation.as_ref())
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn add_event(&self, leaf: &AgentName, event: &Event) -> Result<(), String> {
        let client = self.client.clone();
        let leaf = leaf.clone();
        let event = event.clone();
        tokio::task::spawn_blocking(move || {
            client.add_event(&leaf, &event).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// The leaves of `session` that read at least one column of `event`, in a
/// stable order and without duplicates.
fn leaves_for(routes: &Routes, session: &SessionId, event: &Event) -> Vec<AgentName> {
    let suffixes: BTreeSet<&str> = event
        .fields
        .iter()
        .filter_map(|field| routes.get(&field.column))
        .flatten()
        .map(|suffix| suffix.0.as_str())
        .collect();
    suffixes
        .into_iter()
        .map(|suffix| AgentName::new(format!("{session}-{suffix}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_lib::{EventField, EventValue};

    #[test]
    fn routes_events_to_leaves_reading_their_columns() {
        let routes: Routes = [
            (
                EventColumnName::new("playerStateChange"),
                vec![
                    AgentSuffix("has-existed-4".into()),
                    AgentSuffix("latest-event-to-state-8".into()),
                ],
            ),
            (
                EventColumnName::new("userAction"),
                vec![AgentSuffix("has-existed-within-6".into())],
            ),
        ]
        .into();
        let event = Event {
            time: 1,
            fields: vec![
                EventField::new("playerStateChange", EventValue::StringValue("play".into())),
                EventField::new("cdn", EventValue::StringValue("akamai".into())),
            ],
        };
        assert_eq!(
            leaves_for(&routes, &SessionId::new("s"), &event),
            vec![
                AgentName::new("s-has-existed-4"),
                AgentName::new("s-latest-event-to-state-8"),
            ]
        );
    }
}
