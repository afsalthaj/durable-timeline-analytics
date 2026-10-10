// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Typed calls to the timeline agents.
use common_lib::{
    AgentName, AggregationConfig, AggregationResult, Event, InitializeResult, MetricName,
    SessionId, StatePoint, TimeLineOp, TimelineError,
};

use crate::cli::{AgentRef, AgentType, ClientError, GolemCli, arg};

#[derive(Debug, Clone)]
pub struct TimelineClient {
    cli: GolemCli,
}

impl TimelineClient {
    pub fn new(cli: GolemCli) -> Self {
        TimelineClient { cli }
    }

    pub fn cli(&self) -> &GolemCli {
        &self.cli
    }

    /// Spawns and wires the agents of `metric` for `session`.
    pub fn initialize_timeline(
        &self,
        session: &SessionId,
        metric: &MetricName,
        timeline: &TimeLineOp,
        aggregation: Option<&AggregationConfig>,
    ) -> Result<InitializeResult, ClientError> {
        let driver = AgentRef::new(AgentType::TimelineDriver, AgentName::new(session.as_str()));
        self.cli.invoke(
            &driver,
            "initialize_timeline",
            &[arg(metric)?, arg(timeline)?, arg(&aggregation)?],
        )
    }

    /// Sends an event to a leaf and waits for the push cascade to finish.
    pub fn add_event(&self, leaf: &AgentName, event: &Event) -> Result<(), ClientError> {
        let result: Result<(), TimelineError> =
            self.cli
                .invoke(&event_processor(leaf), "add_event", &[arg(event)?])?;
        Ok(result?)
    }

    /// Sends an event to a leaf without waiting for it to be processed.
    pub fn trigger_add_event(&self, leaf: &AgentName, event: &Event) -> Result<(), ClientError> {
        self.cli
            .trigger(&event_processor(leaf), "add_event", &[arg(event)?])
    }

    pub fn get_leaf_result(
        &self,
        leaf: &AgentName,
        t1: u64,
    ) -> Result<Option<StatePoint>, ClientError> {
        let result: Result<Option<StatePoint>, TimelineError> =
            self.cli
                .invoke(&event_processor(leaf), "get_leaf_result", &[arg(&t1)?])?;
        Ok(result?)
    }

    pub fn get_derived_result(
        &self,
        node: &AgentName,
        t1: u64,
    ) -> Result<Option<StatePoint>, ClientError> {
        let processor = AgentRef::new(AgentType::TimelineProcessor, node.clone());
        let result: Result<Option<StatePoint>, TimelineError> =
            self.cli
                .invoke(&processor, "get_derived_result", &[arg(&t1)?])?;
        Ok(result?)
    }

    /// The aggregate as of the latest event time reported to `aggregator`.
    pub fn get_aggregation_result(
        &self,
        aggregator: &AgentName,
    ) -> Result<AggregationResult, ClientError> {
        let agent = AgentRef::new(AgentType::Aggregator, aggregator.clone());
        self.cli.invoke(&agent, "get_aggregation_result", &[])
    }

    /// The aggregate at event time `time`.
    pub fn get_aggregation_result_at(
        &self,
        aggregator: &AgentName,
        time: u64,
    ) -> Result<AggregationResult, ClientError> {
        let agent = AgentRef::new(AgentType::Aggregator, aggregator.clone());
        self.cli
            .invoke(&agent, "get_aggregation_result_at", &[arg(&time)?])
    }
}

fn event_processor(leaf: &AgentName) -> AgentRef {
    AgentRef::new(AgentType::EventProcessor, leaf.clone())
}
