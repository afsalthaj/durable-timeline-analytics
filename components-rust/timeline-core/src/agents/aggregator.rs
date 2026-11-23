// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use golem_rust::{agent_definition, agent_implementation};

use common_lib::{AgentName, AggregationResult};

use crate::compute::SessionAggregates;
use crate::types::SessionReport;

/// Cross-session aggregator. One Aggregator per metric and group value, e.g.
/// `aggregator-cirr-cdn-akamai`, created lazily when a root first reports.
///
/// Each root reports its session's latest value: fixed, or rising while a
/// duration is accumulating. Results are computed at query time, so
/// `sess-42` buffering since t=100 contributes 50 to `sum` at t=150.
#[agent_definition]
pub trait Aggregator {
    fn new(name: String) -> Self;

    /// Record a session's latest value, replacing what it reported before.
    fn report(&mut self, report: SessionReport);

    /// Forget a session, e.g. because it moved to another group.
    fn remove_session(&mut self, session: AgentName);

    /// Metrics across all sessions as of the latest reported event time.
    fn get_aggregation_result(&self) -> AggregationResult;

    /// Metrics across all sessions at event time `time`.
    fn get_aggregation_result_at(&self, time: u64) -> AggregationResult;
}

struct AggregatorImpl {
    sessions: SessionAggregates,
}

#[agent_implementation]
impl Aggregator for AggregatorImpl {
    fn new(_name: String) -> Self {
        Self {
            sessions: SessionAggregates::default(),
        }
    }

    fn report(&mut self, report: SessionReport) {
        self.sessions.report(report);
    }

    fn remove_session(&mut self, session: AgentName) {
        self.sessions.remove(&session);
    }

    fn get_aggregation_result(&self) -> AggregationResult {
        self.sessions.result()
    }

    fn get_aggregation_result_at(&self, time: u64) -> AggregationResult {
        self.sessions.result_at(time)
    }
}
