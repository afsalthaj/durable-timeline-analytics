// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Feeds events from a Pulsar topic to the leaf agents returned by
//! `TimelineDriver::initialize_timeline`.
//!
//! # Environment variables
//! - `LEAF_AGENTS`: comma-separated leaf agent names (required)
//! - `GOLEM_APP_DIR`: directory containing the app's `golem.yaml` (default: `.`)
//! - `PULSAR_ADDRESS`: broker URL (default: `pulsar://127.0.0.1:6650`)
//! - `PULSAR_TOPIC`: topic (default: `non-persistent://public/default/test`)
//! - `PULSAR_TOKEN` or `PULSAR_OAUTH2`: optional authentication

use std::env;
use std::error::Error;
use std::sync::Arc;

use common_lib::{AgentName, Event};
use futures::TryStreamExt;
use pulsar::authentication::oauth2::OAuth2Authentication;
use pulsar::{
    Authentication, Consumer, DeserializeMessage, Payload, Pulsar, SubType, TokioExecutor,
};
use timeline_client::{GolemCli, TimelineClient};

struct PulsarEvent;

impl DeserializeMessage for PulsarEvent {
    type Output = Result<Event, serde_json::Error>;

    fn deserialize_message(payload: &Payload) -> Self::Output {
        serde_json::from_slice(&payload.data)
    }
}

fn leaf_agents() -> Result<Vec<AgentName>, Box<dyn Error>> {
    let agents: Vec<AgentName> = env::var("LEAF_AGENTS")
        .map_err(|_| "LEAF_AGENTS must be set")?
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(AgentName::new)
        .collect();
    if agents.is_empty() {
        return Err("LEAF_AGENTS must name at least one agent".into());
    }
    Ok(agents)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let leaves = Arc::new(leaf_agents()?);
    let app_dir = env::var("GOLEM_APP_DIR").unwrap_or_else(|_| ".".into());
    let client = TimelineClient::new(GolemCli::new(app_dir));

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
    let mut consumer: Consumer<PulsarEvent, _> = pulsar
        .consumer()
        .with_topic(topic)
        .with_consumer_name("sample-playback-consumer")
        .with_subscription_type(SubType::Exclusive)
        .with_subscription("test_subscription")
        .build()
        .await?;

    while let Some(msg) = consumer.try_next().await? {
        consumer.ack(&msg).await?;
        let event = match msg.deserialize() {
            Ok(event) => event,
            Err(e) => {
                log::error!("skipping undecodable message {:?}: {e}", msg.message_id());
                continue;
            }
        };

        let client = client.clone();
        let leaves = Arc::clone(&leaves);
        tokio::task::spawn_blocking(move || {
            leaves
                .iter()
                .try_for_each(|leaf| client.add_event(leaf, &event))
        })
        .await??;
    }

    Ok(())
}
