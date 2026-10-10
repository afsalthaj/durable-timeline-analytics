// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

/// The name of a column in an event, e.g. `playerStateChange` or `cdn`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    ),
    schema(transparent)
)]
pub struct EventColumnName(pub String);

impl EventColumnName {
    pub fn new(name: impl Into<String>) -> Self {
        EventColumnName(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for EventColumnName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A scalar value carried by an event column, and the state held by timelines.
#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub enum EventValue {
    StringValue(String),
    IntValue(i64),
    FloatValue(f64),
    BoolValue(bool),
}

impl EventValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            EventValue::BoolValue(b) => Some(*b),
            _ => None,
        }
    }

    /// Numeric view of the value, used when aggregating across sessions.
    /// Booleans count as `0.0` / `1.0`; strings have no numeric value.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            EventValue::IntValue(i) => Some(*i as f64),
            EventValue::FloatValue(f) => Some(*f),
            EventValue::BoolValue(b) => Some(if *b { 1.0 } else { 0.0 }),
            EventValue::StringValue(_) => None,
        }
    }
}

impl Display for EventValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventValue::StringValue(s) => f.write_str(s),
            EventValue::IntValue(i) => write!(f, "{i}"),
            EventValue::FloatValue(fl) => write!(f, "{fl}"),
            EventValue::BoolValue(b) => write!(f, "{b}"),
        }
    }
}

/// One column of an event together with its value.
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
pub struct EventField {
    pub column: EventColumnName,
    pub value: EventValue,
}

impl EventField {
    pub fn new(column: impl Into<String>, value: EventValue) -> Self {
        EventField {
            column: EventColumnName::new(column),
            value,
        }
    }
}

/// A timestamped event as ingested by leaf agents.
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
pub struct Event {
    pub time: u64,
    pub fields: Vec<EventField>,
}

impl Event {
    pub fn value_of(&self, column: &EventColumnName) -> Option<&EventValue> {
        self.fields
            .iter()
            .find(|field| &field.column == column)
            .map(|field| &field.value)
    }

    pub fn has_column(&self, column: &EventColumnName) -> bool {
        self.value_of(column).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> Event {
        Event {
            time: 7,
            fields: vec![
                EventField::new("playerStateChange", EventValue::StringValue("play".into())),
                EventField::new("cdn", EventValue::StringValue("akamai".into())),
            ],
        }
    }

    #[test]
    fn value_of_finds_present_column() {
        let event = sample_event();
        assert_eq!(
            event.value_of(&EventColumnName::new("cdn")),
            Some(&EventValue::StringValue("akamai".into()))
        );
    }

    #[test]
    fn value_of_missing_column_is_none() {
        let event = sample_event();
        assert_eq!(event.value_of(&EventColumnName::new("userAction")), None);
        assert!(!event.has_column(&EventColumnName::new("userAction")));
    }

    #[test]
    fn numeric_view_of_values() {
        assert_eq!(EventValue::IntValue(3).as_f64(), Some(3.0));
        assert_eq!(EventValue::FloatValue(2.5).as_f64(), Some(2.5));
        assert_eq!(EventValue::BoolValue(true).as_f64(), Some(1.0));
        assert_eq!(EventValue::BoolValue(false).as_f64(), Some(0.0));
        assert_eq!(EventValue::StringValue("x".into()).as_f64(), None);
    }

    #[test]
    fn event_round_trips_through_json() -> Result<(), serde_json::Error> {
        let event = sample_event();
        let json = serde_json::to_string(&event)?;
        let decoded: Event = serde_json::from_str(&json)?;
        assert_eq!(decoded, event);
        Ok(())
    }
}
