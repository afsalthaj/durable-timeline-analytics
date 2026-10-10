// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Standalone Kafka producer binary.
//!
//! Generates events and publishes them to Kafka.
//! Can be used independently for cloud deployments.
//!
//! # Environment variables
//! - `KAFKA_BROKER`: Kafka broker address (default: "localhost:9092")
//! - `KAFKA_TOPIC`: Kafka topic name (default: "timeline-events")
//! - `SESSION_COUNT`: Number of sessions to generate (default: 10)
//! - `EVENTS_PER_SESSION`: Events per session (default: 100)

use std::error::Error;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn env_number(name: &str, default: usize) -> Result<usize, Box<dyn Error>> {
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|e| format!("{name} must be a number, got '{value}': {e}").into()),
        Err(_) => Ok(default),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let broker = env_or("KAFKA_BROKER", "localhost:9092");
    let topic = env_or("KAFKA_TOPIC", "timeline-events");
    let session_count = env_number("SESSION_COUNT", 10)?;
    let events_per_session = env_number("EVENTS_PER_SESSION", 100)?;

    println!(
        "Kafka producer config: broker={broker}, topic={topic}, sessions={session_count}, events_per_session={events_per_session}"
    );

    let config = event_generator::GeneratorConfig {
        events_per_session,
        session_count,
        ..Default::default()
    };
    let sessions = event_generator::generate_sessions(&config);
    let total_events: usize = sessions.iter().map(|s| s.events.len()).sum();

    let start = std::time::Instant::now();
    let producer = kafka_producer::TimelineKafkaProducer::new(&broker, &topic)?;
    producer.publish_all_sessions(&sessions).await?;

    println!(
        "Published {total_events} events across {} sessions in {:.2?}",
        sessions.len(),
        start.elapsed()
    );
    Ok(())
}
