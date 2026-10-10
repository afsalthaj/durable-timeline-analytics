// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Builders for the timelines and events used across integration tests.
use common_lib::{
    CompareOp, Event, EventColumnName, EventField, EventPredicate, EventValue, TimeLineOp,
};

pub fn string(value: &str) -> EventValue {
    EventValue::StringValue(value.to_string())
}

/// An event with string-valued columns, e.g. `event(100, &[("cdn", "akamai")])`.
pub fn event(time: u64, columns: &[ColumnValue<'_>]) -> Event {
    Event {
        time,
        fields: columns
            .iter()
            .map(|c| EventField::new(c.column, string(c.value)))
            .collect(),
    }
}

/// One string-valued column of a test event.
#[derive(Clone, Copy, Debug)]
pub struct ColumnValue<'a> {
    pub column: &'a str,
    pub value: &'a str,
}

pub fn column<'a>(column: &'a str, value: &'a str) -> ColumnValue<'a> {
    ColumnValue { column, value }
}

pub fn latest(column: &str) -> TimeLineOp {
    TimeLineOp::TlLatestEventToState(EventColumnName::new(column))
}

pub fn has_existed(column: &str, value: &str) -> TimeLineOp {
    TimeLineOp::TlHasExisted(EventPredicate::equals(column, string(value)))
}

/// The CIRR (connection-induced rebuffering ratio) numerator:
/// ```text
/// duration_where(
///   has_existed(playerStateChange == "play")
///   && !has_existed_within(playerStateChange == "seek", 5)
///   && latest_event_to_state(playerStateChange) == "buffer"
/// )
/// ```
pub fn cirr() -> TimeLineOp {
    TimeLineOp::duration_where(TimeLineOp::and(
        TimeLineOp::and(
            has_existed("playerStateChange", "play"),
            !(TimeLineOp::TlHasExistedWithin {
                predicate: EventPredicate::equals("playerStateChange", string("seek")),
                within: 5,
            }),
        ),
        TimeLineOp::compare(
            CompareOp::EqualTo,
            latest("playerStateChange"),
            string("buffer"),
        ),
    ))
}
