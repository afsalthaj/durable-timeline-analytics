// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use golem_rust::{agent_definition, agent_implementation};

use common_lib::{AgentName, AggregationConfig, InitializeResult, SessionId, TimeLineOp};

use crate::agents::event_processor::EventProcessorClient;
use crate::agents::timeline_processor::TimelineProcessorClient;
use crate::plan::{plan_agents, AgentPlan, NodeRole};

/// Orchestrator agent. Spawns all leaf and derived agents for a timeline
/// expression and wires them together. One driver per session.
///
/// CIRR example for session `sess-42`:
///   ```text
///   duration_where(
///     has_existed(playerStateChange == "play")
///     && !has_existed_within(playerStateChange == "seek", 5)
///     && latest_event_to_state(playerStateChange) == "buffer"
///   ) | aggregate(group_by=cdn, count, sum, avg)
///   ```
///
///   The driver spawns 8 agents (pre-order depth-first numbering):
///     sess-42-duration-where-1         DurationWhere(and-2)    ← root
///     sess-42-and-2                    And(and-3, equal-to-7)
///     sess-42-and-3                    And(has-existed-4, not-5)
///     sess-42-has-existed-4            leaf
///     sess-42-not-5                    Not(has-existed-within-6)
///     sess-42-has-existed-within-6     leaf
///     sess-42-equal-to-7               EqualTo(latest-event-to-state-8, "buffer")
///     sess-42-latest-event-to-state-8  leaf
///
///   With aggregation, every leaf extracts `cdn` from its events and the root
///   reports to `aggregator-cdn-{value}`.
#[agent_definition]
pub trait TimelineDriver {
    fn new(name: String) -> Self;

    /// Spawn all agents for a timeline expression and wire them together.
    async fn initialize_timeline(
        &self,
        timeline: TimeLineOp,
        aggregation: Option<AggregationConfig>,
    ) -> InitializeResult;
}

struct TimelineDriverImpl {
    session: SessionId,
}

#[agent_implementation]
impl TimelineDriver for TimelineDriverImpl {
    fn new(name: String) -> Self {
        Self {
            session: SessionId::new(name),
        }
    }

    async fn initialize_timeline(
        &self,
        timeline: TimeLineOp,
        aggregation: Option<AggregationConfig>,
    ) -> InitializeResult {
        let plans = plan_agents(&self.session, &timeline);
        for plan in &plans {
            setup_agent(plan, aggregation.as_ref()).await;
        }

        let leaf_agents = plans
            .iter()
            .filter(|plan| plan.is_leaf())
            .map(|plan| plan.agent_name.clone())
            .collect();
        let root_agent = plans
            .first()
            .map(|plan| plan.agent_name.clone())
            .unwrap_or_else(|| AgentName::new(self.session.as_str()));

        InitializeResult {
            root_agent,
            leaf_agents,
        }
    }
}

async fn setup_agent(plan: &AgentPlan, aggregation: Option<&AggregationConfig>) {
    let name = plan.agent_name.0.clone();
    match &plan.role {
        NodeRole::Leaf(operation) => {
            let mut client = EventProcessorClient::get(name);
            client.initialize_leaf(operation.clone()).await;
            if let Some(parent) = &plan.parent {
                client.set_parent(parent.clone()).await;
            }
            if let Some(config) = aggregation {
                client
                    .set_group_by_column(config.group_by_column.clone())
                    .await;
            }
        }
        NodeRole::Derived(operation) => {
            let mut client = TimelineProcessorClient::get(name);
            client.initialize_derived(operation.clone()).await;
            match &plan.parent {
                Some(parent) => client.set_parent(parent.clone()).await,
                None => {
                    if let Some(config) = aggregation {
                        client.set_aggregation(config.clone()).await;
                    }
                }
            }
        }
    }
}
