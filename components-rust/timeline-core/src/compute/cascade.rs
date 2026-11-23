// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! End-to-end evaluation of the CIRR expression without a Golem cluster.
use common_lib::{CompareOp, Event, EventColumnName, EventField, EventPredicate, EventValue};

use super::derived::{ChildUpdate, DerivedState};
use super::leaf::{LeafChange, LeafState};
use crate::types::{ChildSlot, DerivedOperation, LeafOperation};

fn string(value: &str) -> EventValue {
    EventValue::StringValue(value.to_string())
}

fn player_event(time: u64, state: &str) -> Event {
    Event {
        time,
        fields: vec![EventField::new("playerStateChange", string(state))],
    }
}

struct Cirr {
    played: LeafState,
    recent_seek: LeafState,
    latest: LeafState,
    no_recent_seek: DerivedState,
    played_and_settled: DerivedState,
    is_buffer: DerivedState,
    condition: DerivedState,
    duration: DerivedState,
}

impl Cirr {
    fn new() -> Self {
        let play = EventPredicate::equals("playerStateChange", string("play"));
        let seek = EventPredicate::equals("playerStateChange", string("seek"));
        Cirr {
            played: LeafState::new(LeafOperation::HasExisted(play)),
            recent_seek: LeafState::new(LeafOperation::HasExistedWithin {
                predicate: seek,
                within: 5,
            }),
            latest: LeafState::new(LeafOperation::LatestEventToState(EventColumnName::new(
                "playerStateChange",
            ))),
            no_recent_seek: DerivedState::new(DerivedOperation::Not),
            played_and_settled: DerivedState::new(DerivedOperation::And),
            is_buffer: DerivedState::new(DerivedOperation::Compare {
                op: CompareOp::EqualTo,
                value: string("buffer"),
            }),
            condition: DerivedState::new(DerivedOperation::And),
            duration: DerivedState::new(DerivedOperation::DurationWhere),
        }
    }

    fn on_event(&mut self, event: &Event) {
        let played = self.played.ingest(event);
        let seek = self.recent_seek.ingest(event);
        let latest = self.latest.ingest(event);

        let no_seek = feed(&mut self.no_recent_seek, ChildSlot::Left, seek);
        let mut settled = feed(&mut self.played_and_settled, ChildSlot::Left, played);
        settled.extend(feed(
            &mut self.played_and_settled,
            ChildSlot::Right,
            no_seek,
        ));
        let buffering = feed(&mut self.is_buffer, ChildSlot::Left, latest);
        let mut condition = feed(&mut self.condition, ChildSlot::Left, settled);
        condition.extend(feed(&mut self.condition, ChildSlot::Right, buffering));
        feed(&mut self.duration, ChildSlot::Left, condition);
    }

    fn duration_at(&self, t: u64) -> Option<EventValue> {
        self.duration.state_at(t).map(|point| point.value)
    }
}

fn feed(node: &mut DerivedState, slot: ChildSlot, changes: Vec<LeafChange>) -> Vec<LeafChange> {
    let mut forwarded = Vec::new();
    for change in changes {
        for produced in node.apply(ChildUpdate {
            slot,
            time: change.time,
            value: change.value,
        }) {
            forwarded.push(LeafChange {
                time: produced.time,
                value: produced.value,
            });
        }
    }
    forwarded
}

/// Seek at t=200 opens a 5-second window. Buffering starts at t=202, inside
/// that window, and is still going at t=250. The seek stops counting at t=205,
/// so connection-induced rebuffering from 205 to 250 is 45 — not 0.
#[test]
fn cirr_counts_buffering_that_starts_inside_the_seek_window() {
    let mut cirr = Cirr::new();
    cirr.on_event(&player_event(100, "play"));
    cirr.on_event(&player_event(200, "seek"));
    cirr.on_event(&player_event(202, "buffer"));

    assert_eq!(cirr.duration_at(202), Some(EventValue::IntValue(0)));
    assert_eq!(cirr.duration_at(205), Some(EventValue::IntValue(0)));
    assert_eq!(cirr.duration_at(250), Some(EventValue::IntValue(45)));
}
