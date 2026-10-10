// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Event generator for timeline analytics testing.
//!
//! Generates realistic event streams (e.g. CIRR playback events) that can be
//! published to Kafka for both local integration tests and cloud deployments.
//!
//! # Usage
//! ```rust
//! let events = event_generator::generate_cirr_session_events("sess-1", "akamai");
//! assert_eq!(events.len(), 100);
//! ```

pub use common_lib::{Event, EventColumnName, EventField, EventValue};

pub const PLAYER_STATE_CHANGE: &str = "playerStateChange";
pub const USER_ACTION: &str = "userAction";
pub const CDN: &str = "cdn";

/// Configuration for event generation.
pub struct GeneratorConfig {
    /// Number of events to generate per session.
    pub events_per_session: usize,
    /// Number of sessions to generate.
    pub session_count: usize,
    /// CDN values to distribute sessions across (for aggregation testing).
    pub cdn_values: Vec<String>,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            events_per_session: 100,
            session_count: 10,
            cdn_values: vec!["akamai".into(), "cloudfront".into(), "fastly".into()],
        }
    }
}

/// A generated session containing its ID, CDN, and event stream.
pub struct GeneratedSession {
    pub session_id: String,
    pub cdn: String,
    pub events: Vec<Event>,
}

/// States of the simulated video player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerState {
    Init,
    Buffer,
    Play,
    Pause,
}

impl PlayerState {
    pub fn as_str(self) -> &'static str {
        match self {
            PlayerState::Init => "init",
            PlayerState::Buffer => "buffer",
            PlayerState::Play => "play",
            PlayerState::Pause => "pause",
        }
    }
}

/// Generate CIRR-style playback events for a single session.
///
/// Events include playerStateChange (init, buffer, play, pause) and
/// userAction seek events, with realistic timing.
pub fn generate_cirr_session_events(session_id: &str, cdn: &str) -> Vec<Event> {
    generate_cirr_session_events_n(session_id, cdn, 100)
}

fn hash_session(session_id: &str) -> u64 {
    session_id.bytes().fold(5381u64, |h, b| {
        h.wrapping_mul(33).wrapping_add(u64::from(b))
    })
}

/// Deterministic pseudo-random sequence seeded from the session ID, so a
/// session always produces the same events.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    /// A time step of 1–3 units.
    fn time_step(&mut self) -> u64 {
        (self.next() % 3) + 1
    }
}

fn event(time: u64, column: &str, value: &str, cdn: &str) -> Event {
    Event {
        time,
        fields: vec![
            EventField::new(column, EventValue::StringValue(value.into())),
            EventField::new(CDN, EventValue::StringValue(cdn.into())),
        ],
    }
}

fn player_event(time: u64, state: PlayerState, cdn: &str) -> Event {
    event(time, PLAYER_STATE_CHANGE, state.as_str(), cdn)
}

/// Generate `count` CIRR-style events for one session.
pub fn generate_cirr_session_events_n(session_id: &str, cdn: &str, count: usize) -> Vec<Event> {
    let mut events = Vec::with_capacity(count);
    if count == 0 {
        return events;
    }
    let mut rng = Lcg(hash_session(session_id));
    let mut time: u64 = 1;

    events.push(player_event(time, PlayerState::Init, cdn));
    time += 1;

    // After init the player buffers first.
    let mut current = PlayerState::Buffer;
    while events.len() < count {
        time += rng.time_step();

        let next = match current {
            PlayerState::Buffer | PlayerState::Init => PlayerState::Play,
            PlayerState::Play => match rng.next() % 10 {
                0 | 1 => PlayerState::Buffer,
                2 | 3 => PlayerState::Pause,
                4 => {
                    events.push(event(time, USER_ACTION, "seek", cdn));
                    if events.len() >= count {
                        break;
                    }
                    time += rng.time_step();
                    PlayerState::Play
                }
                _ => PlayerState::Play,
            },
            PlayerState::Pause => {
                if rng.next().is_multiple_of(3) {
                    PlayerState::Buffer
                } else {
                    PlayerState::Play
                }
            }
        };

        current = next;
        events.push(player_event(time, current, cdn));
    }

    events
}

/// Generate multiple sessions worth of events according to the config.
///
/// Sessions are distributed across CDN values round-robin.
pub fn generate_sessions(config: &GeneratorConfig) -> Vec<GeneratedSession> {
    (0..config.session_count)
        .filter_map(|i| {
            let cdn = config.cdn_values.get(i % config.cdn_values.len().max(1))?;
            let session_id = format!("sess-{i}");
            let events =
                generate_cirr_session_events_n(&session_id, cdn, config.events_per_session);
            Some(GeneratedSession {
                session_id,
                cdn: cdn.clone(),
                events,
            })
        })
        .collect()
}

/// The last `playerStateChange` value in `events` and when it happened.
pub fn last_player_state(events: &[Event]) -> Option<LastPlayerState> {
    let column = EventColumnName::new(PLAYER_STATE_CHANGE);
    events.iter().rev().find_map(|event| {
        event.value_of(&column).map(|value| LastPlayerState {
            time: event.time,
            value: value.clone(),
        })
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct LastPlayerState {
    pub time: u64,
    pub value: EventValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_exactly_count_events() {
        for count in [0, 1, 2, 10, 100, 1000] {
            assert_eq!(
                generate_cirr_session_events_n("s", "akamai", count).len(),
                count
            );
        }
    }

    #[test]
    fn is_deterministic_per_session() {
        let a = generate_cirr_session_events_n("sess-1", "akamai", 200);
        let b = generate_cirr_session_events_n("sess-1", "akamai", 200);
        let c = generate_cirr_session_events_n("sess-2", "akamai", 200);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn time_increases_and_every_event_has_a_cdn() {
        let events = generate_cirr_session_events_n("sess-1", "fastly", 500);
        let cdn = EventColumnName::new(CDN);
        assert!(events.windows(2).all(|w| match w {
            [a, b] => a.time < b.time,
            _ => true,
        }));
        assert!(
            events
                .iter()
                .all(|e| e.value_of(&cdn) == Some(&EventValue::StringValue("fastly".into())))
        );
    }

    #[test]
    fn starts_with_init() {
        let events = generate_cirr_session_events_n("sess-1", "akamai", 5);
        assert_eq!(
            events
                .first()
                .and_then(|e| e.value_of(&EventColumnName::new(PLAYER_STATE_CHANGE))),
            Some(&EventValue::StringValue("init".into()))
        );
    }

    #[test]
    fn sessions_round_robin_cdns() {
        let sessions = generate_sessions(&GeneratorConfig {
            events_per_session: 3,
            session_count: 4,
            cdn_values: vec!["a".into(), "b".into()],
        });
        let cdns: Vec<&str> = sessions.iter().map(|s| s.cdn.as_str()).collect();
        assert_eq!(cdns, vec!["a", "b", "a", "b"]);
    }

    #[test]
    fn finds_last_player_state() {
        let events = vec![
            player_event(1, PlayerState::Init, "x"),
            player_event(3, PlayerState::Play, "x"),
            event(4, USER_ACTION, "seek", "x"),
        ];
        assert_eq!(
            last_player_state(&events),
            Some(LastPlayerState {
                time: 3,
                value: EventValue::StringValue("play".into())
            })
        );
    }
}
