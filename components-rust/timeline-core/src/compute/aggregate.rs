// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::collections::BTreeMap;

use common_lib::{AgentName, AggregationResult};

use crate::types::{SessionReport, SessionValue};

/// The latest value of every session in one aggregation group. Results are
/// computed at query time, so sessions whose value is still rising are
/// included up to the queried time.
#[derive(Default)]
pub struct SessionAggregates {
    sessions: BTreeMap<AgentName, SessionValue>,
    latest_time: u64,
}

impl SessionAggregates {
    pub fn report(&mut self, report: SessionReport) {
        self.latest_time = self.latest_time.max(report.time);
        self.sessions.insert(report.session, report.value);
    }

    pub fn remove(&mut self, session: &AgentName) {
        self.sessions.remove(session);
    }

    /// The aggregate as of the latest reported event time.
    pub fn result(&self) -> AggregationResult {
        self.result_at(self.latest_time)
    }

    pub fn result_at(&self, time: u64) -> AggregationResult {
        let values: Vec<f64> = self
            .sessions
            .values()
            .map(|value| value.value_at(time))
            .collect();
        let count = values.len() as u64;
        let sum: f64 = values.iter().sum();
        AggregationResult {
            count,
            sum,
            avg: if count == 0 { 0.0 } else { sum / count as f64 },
            min: values.iter().copied().reduce(f64::min),
            max: values.iter().copied().reduce(f64::max),
        }
    }
}

/// Where a root node's report should go.
#[derive(Clone, Debug, PartialEq)]
pub struct ReportRoute {
    pub report_to: AgentName,
    /// The aggregator the session previously reported to, if its group changed.
    pub remove_from: Option<AgentName>,
}

/// Tracks which aggregator a root node reports to and what it last reported.
#[derive(Default)]
pub struct AggregatorLink {
    aggregator: Option<AgentName>,
    last_reported: Option<SessionValue>,
}

impl AggregatorLink {
    /// Records that the session's value is now `value` in the group served by
    /// `aggregator`. Returns where to send it, or `None` if that aggregator
    /// already has this value.
    pub fn route(&mut self, aggregator: AgentName, value: SessionValue) -> Option<ReportRoute> {
        let same_group = self.aggregator.as_ref() == Some(&aggregator);
        if same_group && self.last_reported == Some(value) {
            return None;
        }
        let remove_from = if same_group {
            None
        } else {
            self.aggregator.take()
        };
        self.aggregator = Some(aggregator.clone());
        self.last_reported = Some(value);
        Some(ReportRoute {
            report_to: aggregator,
            remove_from,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(session: &str, time: u64, value: SessionValue) -> SessionReport {
        SessionReport {
            session: AgentName::new(session),
            time,
            value,
        }
    }

    fn fixed(value: f64) -> SessionValue {
        SessionValue::Fixed { value }
    }

    #[test]
    fn empty_group_has_no_sessions() {
        let result = SessionAggregates::default().result();
        assert_eq!(
            result,
            AggregationResult {
                count: 0,
                sum: 0.0,
                avg: 0.0,
                min: None,
                max: None,
            }
        );
    }

    #[test]
    fn sessions_with_zero_value_are_counted() {
        let mut group = SessionAggregates::default();
        group.report(report("a", 10, fixed(0.0)));
        group.report(report("b", 20, fixed(4.0)));
        let result = group.result();
        assert_eq!(result.count, 2);
        assert_eq!(result.sum, 4.0);
        assert_eq!(result.avg, 2.0);
        assert_eq!(result.min, Some(0.0));
        assert_eq!(result.max, Some(4.0));
    }

    #[test]
    fn later_report_replaces_session_value() {
        let mut group = SessionAggregates::default();
        group.report(report("a", 10, fixed(1.0)));
        group.report(report("a", 20, fixed(3.0)));
        let result = group.result();
        assert_eq!(result.count, 1);
        assert_eq!(result.sum, 3.0);
    }

    #[test]
    fn rising_sessions_count_up_to_query_time() {
        let mut group = SessionAggregates::default();
        group.report(report(
            "a",
            100,
            SessionValue::Rising {
                base: 5.0,
                since: 100,
            },
        ));
        group.report(report("b", 130, fixed(10.0)));
        assert_eq!(group.result().sum, 45.0);
        assert_eq!(group.result_at(150).sum, 65.0);
        assert_eq!(group.result_at(150).max, Some(55.0));
    }

    #[test]
    fn removed_session_no_longer_counts() {
        let mut group = SessionAggregates::default();
        group.report(report("a", 1, fixed(1.0)));
        group.report(report("b", 1, fixed(2.0)));
        group.remove(&AgentName::new("a"));
        assert_eq!(group.result().count, 1);
        assert_eq!(group.result().sum, 2.0);
    }

    #[test]
    fn link_reports_first_value_and_changes_only() {
        let mut link = AggregatorLink::default();
        let akamai = AgentName::new("aggregator-m-cdn-akamai");
        assert_eq!(
            link.route(akamai.clone(), fixed(0.0)),
            Some(ReportRoute {
                report_to: akamai.clone(),
                remove_from: None,
            })
        );
        assert_eq!(link.route(akamai.clone(), fixed(0.0)), None);
        assert!(link.route(akamai, fixed(1.0)).is_some());
    }

    #[test]
    fn link_moves_session_when_group_changes() {
        let mut link = AggregatorLink::default();
        let akamai = AgentName::new("aggregator-m-cdn-akamai");
        let fastly = AgentName::new("aggregator-m-cdn-fastly");
        link.route(akamai.clone(), fixed(1.0));
        assert_eq!(
            link.route(fastly.clone(), fixed(1.0)),
            Some(ReportRoute {
                report_to: fastly,
                remove_from: Some(akamai),
            })
        );
    }
}
