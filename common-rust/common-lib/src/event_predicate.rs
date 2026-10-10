// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

use crate::event::{Event, EventColumnName, EventValue};

/// A condition evaluated against a single event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub enum EventPredicate {
    Equals {
        column: EventColumnName,
        value: EventValue,
    },
    GreaterThan {
        column: EventColumnName,
        value: EventValue,
    },
    LessThan {
        column: EventColumnName,
        value: EventValue,
    },
    And {
        left: Box<EventPredicate>,
        right: Box<EventPredicate>,
    },
    Or {
        left: Box<EventPredicate>,
        right: Box<EventPredicate>,
    },
}

impl EventPredicate {
    pub fn equals(column: impl Into<String>, value: EventValue) -> Self {
        EventPredicate::Equals {
            column: EventColumnName::new(column),
            value,
        }
    }

    pub fn greater_than(column: impl Into<String>, value: EventValue) -> Self {
        EventPredicate::GreaterThan {
            column: EventColumnName::new(column),
            value,
        }
    }

    pub fn less_than(column: impl Into<String>, value: EventValue) -> Self {
        EventPredicate::LessThan {
            column: EventColumnName::new(column),
            value,
        }
    }

    pub fn and(self, other: EventPredicate) -> Self {
        EventPredicate::And {
            left: Box::new(self),
            right: Box::new(other),
        }
    }

    pub fn or(self, other: EventPredicate) -> Self {
        EventPredicate::Or {
            left: Box::new(self),
            right: Box::new(other),
        }
    }

    pub fn evaluate(&self, event: &Event) -> bool {
        match self {
            EventPredicate::Equals { column, value } => {
                event.value_of(column).is_some_and(|v| v == value)
            }
            EventPredicate::GreaterThan { column, value } => {
                event.value_of(column).is_some_and(|v| v > value)
            }
            EventPredicate::LessThan { column, value } => {
                event.value_of(column).is_some_and(|v| v < value)
            }
            EventPredicate::And { left, right } => left.evaluate(event) && right.evaluate(event),
            EventPredicate::Or { left, right } => left.evaluate(event) || right.evaluate(event),
        }
    }

    /// Every event column this predicate reads, in first-seen order.
    pub fn columns(&self) -> Vec<&EventColumnName> {
        let mut columns = Vec::new();
        self.collect_columns(&mut columns);
        columns
    }

    fn collect_columns<'a>(&'a self, columns: &mut Vec<&'a EventColumnName>) {
        match self {
            EventPredicate::Equals { column, .. }
            | EventPredicate::GreaterThan { column, .. }
            | EventPredicate::LessThan { column, .. } => {
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
            EventPredicate::And { left, right } | EventPredicate::Or { left, right } => {
                left.collect_columns(columns);
                right.collect_columns(columns);
            }
        }
    }
}

impl Display for EventPredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventPredicate::Equals { column, value } => write!(f, "{column} == {value}"),
            EventPredicate::GreaterThan { column, value } => write!(f, "{column} > {value}"),
            EventPredicate::LessThan { column, value } => write!(f, "{column} < {value}"),
            EventPredicate::And { left, right } => write!(f, "{left} && {right}"),
            EventPredicate::Or { left, right } => write!(f, "{left} || {right}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventField;

    fn event_with(fields: Vec<EventField>) -> Event {
        Event { time: 1, fields }
    }

    #[test]
    fn equals_matches_only_same_value() {
        let predicate = EventPredicate::equals("status", EventValue::StringValue("play".into()));
        assert!(predicate.evaluate(&event_with(vec![EventField::new(
            "status",
            EventValue::StringValue("play".into())
        )])));
        assert!(!predicate.evaluate(&event_with(vec![EventField::new(
            "status",
            EventValue::StringValue("pause".into())
        )])));
    }

    #[test]
    fn missing_column_never_matches() {
        let predicate = EventPredicate::greater_than("score", EventValue::IntValue(1));
        assert!(!predicate.evaluate(&event_with(vec![])));
    }

    #[test]
    fn ordering_predicates() {
        let event = event_with(vec![EventField::new("score", EventValue::IntValue(10))]);
        assert!(EventPredicate::greater_than("score", EventValue::IntValue(5)).evaluate(&event));
        assert!(!EventPredicate::greater_than("score", EventValue::IntValue(10)).evaluate(&event));
        assert!(EventPredicate::less_than("score", EventValue::IntValue(11)).evaluate(&event));
    }

    #[test]
    fn compound_predicates() {
        let event = event_with(vec![
            EventField::new("a", EventValue::IntValue(1)),
            EventField::new("b", EventValue::IntValue(2)),
        ]);
        let a_is_1 = EventPredicate::equals("a", EventValue::IntValue(1));
        let b_is_3 = EventPredicate::equals("b", EventValue::IntValue(3));
        assert!(!a_is_1.clone().and(b_is_3.clone()).evaluate(&event));
        assert!(a_is_1.or(b_is_3).evaluate(&event));
    }

    #[test]
    fn columns_are_deduplicated() {
        let predicate = EventPredicate::equals("a", EventValue::IntValue(1))
            .or(EventPredicate::equals("b", EventValue::IntValue(2)))
            .and(EventPredicate::less_than("a", EventValue::IntValue(5)));
        let names: Vec<&str> = predicate.columns().iter().map(|c| c.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn display_is_dsl_like() {
        let predicate = EventPredicate::equals("status", EventValue::StringValue("on".into()))
            .and(EventPredicate::less_than("x", EventValue::IntValue(3)));
        assert_eq!(predicate.to_string(), "status == on && x < 3");
    }
}
