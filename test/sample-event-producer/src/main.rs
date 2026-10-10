// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Publishes a sample CIRR playback session to a Pulsar topic.
//!
//! # Environment variables
//! - `PULSAR_ADDRESS`: broker URL (default: `pulsar://127.0.0.1:6650`)
//! - `PULSAR_TOPIC`: topic (default: `non-persistent://public/default/test`)
//! - `PULSAR_TOKEN` or `PULSAR_OAUTH2`: optional authentication

use std::env;
use std::error::Error;
use std::time::Duration;

use common_lib::{Event, EventField, EventValue};
use pulsar::authentication::oauth2::OAuth2Authentication;
use pulsar::{
    Authentication, Error as PulsarError, Pulsar, SerializeMessage, TokioExecutor, producer,
};

struct PulsarEvent(Event);

impl SerializeMessage for PulsarEvent {
    fn serialize_message(input: Self) -> Result<producer::Message, PulsarError> {
        let payload =
            serde_json::to_vec(&input.0).map_err(|e| PulsarError::Custom(e.to_string()))?;
        Ok(producer::Message {
            payload,
            ..Default::default()
        })
    }
}

fn event(time: u64, column: &str, value: &str) -> Event {
    Event {
        time,
        fields: vec![EventField::new(
            column,
            EventValue::StringValue(value.to_string()),
        )],
    }
}

fn cirr_session() -> Vec<Event> {
    vec![
        event(1, "playerStateChange", "init"),
        event(2, "cdn", "CDN1"),
        event(4, "playerStateChange", "buffer"),
        event(5, "playerStateChange", "play"),
        event(6, "playerStateChange", "buffer"),
        event(7, "userAction", "seek"),
        event(9, "playerStateChange", "play"),
        event(10, "playerStateChange", "pause"),
        event(11, "cdn", "CDN2"),
        event(13, "playerStateChange", "buffer"),
        event(14, "playerStateChange", "play"),
    ]
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let addr = env::var("PULSAR_ADDRESS").unwrap_or_else(|_| "pulsar://127.0.0.1:6650".into());
    let topic =
        env::var("PULSAR_TOPIC").unwrap_or_else(|_| "non-persistent://public/default/test".into());

    let mut builder = Pulsar::builder(addr, TokioExecutor);
    if let Ok(token) = env::var("PULSAR_TOKEN") {
        builder = builder.with_auth(Authentication {
            name: "token".to_string(),
            data: token.into_bytes(),
        });
    } else if let Ok(oauth2_cfg) = env::var("PULSAR_OAUTH2") {
        let params = serde_json::from_str(&oauth2_cfg)
            .map_err(|e| format!("invalid PULSAR_OAUTH2 config: {e}"))?;
        builder = builder.with_auth_provider(OAuth2Authentication::client_credentials(params));
    }

    let pulsar: Pulsar<_> = builder.build().await?;
    let mut producer = pulsar
        .producer()
        .with_topic(topic)
        .with_name("sample-playback-events")
        .build()
        .await?;

    for event in cirr_session() {
        log::info!("Sending event: {event:?}");
        producer
            .send_non_blocking(PulsarEvent(event))
            .await?
            .await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}
