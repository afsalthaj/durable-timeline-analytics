// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use golem_rust::{agent_definition, agent_implementation};

use common_lib::{AggregationFunction, AggregationResult};

/// Cross-session accumulator. One Aggregator per (column, value) pair, e.g.
/// `aggregator-cdn-akamai`, created lazily by root TimelineProcessors.
///
/// Session `sess-42` (cdn = "akamai") reports a CIRR delta of 3.0 →
/// `Aggregator("aggregator-cdn-akamai").on_delta(3.0)`. Session `sess-99`
/// (also akamai) reports 1.5 to the same agent. Querying it returns e.g.
/// `{ count: 5000, sum: 12000.0, avg: 2.4 }`.
#[agent_definition]
pub trait Aggregator {
    fn new(name: String) -> Self;

    /// Set which aggregation functions to compute.
    fn initialize_aggregator(&mut self, functions: Vec<AggregationFunction>);

    /// Receive a change in one session's value.
    fn on_delta(&mut self, delta: f64);

    /// Register a new session contributing to this aggregator.
    fn register_session(&mut self);

    /// Query accumulated metrics across all sessions for this group.
    fn get_aggregation_result(&self) -> AggregationResult;
}

struct AggregatorImpl {
    functions: Vec<AggregationFunction>,
    count: u64,
    sum: f64,
}

#[agent_implementation]
impl Aggregator for AggregatorImpl {
    fn new(_name: String) -> Self {
        Self {
            functions: Vec::new(),
            count: 0,
            sum: 0.0,
        }
    }

    fn initialize_aggregator(&mut self, functions: Vec<AggregationFunction>) {
        self.functions = functions;
    }

    fn on_delta(&mut self, delta: f64) {
        self.sum += delta;
    }

    fn register_session(&mut self) {
        self.count += 1;
    }

    fn get_aggregation_result(&self) -> AggregationResult {
        let avg = if self.count > 0 {
            self.sum / self.count as f64
        } else {
            0.0
        };
        AggregationResult {
            count: self.count,
            sum: self.sum,
            avg,
            min: None,
            max: None,
        }
    }
}
