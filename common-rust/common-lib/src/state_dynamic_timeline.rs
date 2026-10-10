// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::collections::BTreeMap;
use std::fmt::Debug;

use crate::event_timeline::EventTimeLine;
use crate::internals::aligned_state_dynamic_timeline::AlignedStateDynamicsTimeLine;
use crate::internals::boundaries::Boundaries;
use crate::internals::zip_result::ZipResult;
use crate::state_dynamic_timeline_point::StateDynamicsTimeLinePoint;

#[derive(Clone, Debug, PartialEq)]
pub struct StateDynamicsTimeLine<T> {
    pub points: BTreeMap<u64, StateDynamicsTimeLinePoint<T>>,
}

impl<T: Clone + PartialEq> StateDynamicsTimeLine<T> {
    pub fn map<B>(&self, f: impl Fn(&T) -> B) -> StateDynamicsTimeLine<B> {
        let mut new_points = BTreeMap::new();
        for (k, v) in &self.points {
            new_points.insert(
                *k,
                StateDynamicsTimeLinePoint {
                    t1: v.t1,
                    t2: v.t2,
                    value: f(&v.value),
                },
            );
        }
        StateDynamicsTimeLine { points: new_points }
    }

    pub fn map_fallible<B>(
        &self,
        f: impl Fn(&T) -> Result<B, String>,
    ) -> Result<StateDynamicsTimeLine<B>, String> {
        let mut new_points = BTreeMap::new();
        for (k, v) in &self.points {
            new_points.insert(
                *k,
                StateDynamicsTimeLinePoint {
                    t1: v.t1,
                    t2: v.t2,
                    value: f(&v.value)?,
                },
            );
        }
        Ok(StateDynamicsTimeLine { points: new_points })
    }

    pub fn from(vec: Vec<StateDynamicsTimeLinePoint<T>>) -> StateDynamicsTimeLine<T> {
        let mut points = BTreeMap::new();
        for point in vec {
            points.insert(point.t1, point);
        }
        StateDynamicsTimeLine { points }
    }

    pub fn last(&self) -> Option<&StateDynamicsTimeLinePoint<T>> {
        self.points.last_key_value().map(|(_, v)| v)
    }

    pub fn last_value_is(&self, value: T) -> bool {
        self.last().map(|x| x.value == value).unwrap_or(false)
    }

    pub fn future_is(&self, value: T) -> bool {
        self.last()
            .map(|x| x.t2.is_none() && x.value == value)
            .unwrap_or(false)
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn get_state_at(&self, t: u64) -> Option<&StateDynamicsTimeLinePoint<T>> {
        self.points.range(..t).next_back().map(|(_, v)| v)
    }

    pub fn boundary(&self, t: u64) -> Option<u64> {
        let previous_point = self.points.range(..t).next_back();

        let next_point = self.points.range(t..).next();

        match (previous_point, next_point) {
            (Some((_, left)), Some((_, _))) => Some(left.t1),
            (Some((_, left)), None) => {
                if left.contains(t) {
                    Some(left.t1)
                } else {
                    None
                }
            }
            (None, Some((_, right))) => {
                if right.contains(t) {
                    Some(right.t1)
                } else {
                    None
                }
            }
            (None, None) => None,
        }
    }

    // Aka building tl_latest_event_to_state
    pub fn add_state_dynamic_info(&mut self, new_time: u64, value: T) {
        // Extract only the needed scalar data from neighbors (avoids cloning the entire BTreeMap).
        // For prev: we need t1, t2, and the value (cloned once).
        // For next: we only need t1, t2, and whether the value equals the new one.
        let prev = self
            .points
            .range(..new_time)
            .next_back()
            .map(|(_, p)| (p.t1, p.t2, p.value.clone()));
        let next = self
            .points
            .range(new_time..)
            .next()
            .map(|(_, p)| (p.t1, p.t2, p.value == value));

        match (prev, next) {
            // The new time is in between timelines.
            (Some((left_t1, left_t2, left_val)), Some(_)) => {
                if left_val != value {
                    self.points.insert(
                        left_t1,
                        StateDynamicsTimeLinePoint {
                            t1: left_t1,
                            t2: Some(new_time),
                            value: left_val,
                        },
                    );
                    self.points.insert(
                        new_time,
                        StateDynamicsTimeLinePoint {
                            t1: new_time,
                            t2: left_t2,
                            value,
                        },
                    );
                }
            }

            // the new event falls on the right side of the existing time line
            (Some((left_t1, left_t2, left_val)), None) => {
                if left_val == value && left_t2.map_or(false, |t2| t2 < new_time) {
                    self.points.insert(
                        left_t1,
                        StateDynamicsTimeLinePoint {
                            t1: left_t1,
                            t2: None,
                            value: left_val,
                        },
                    );
                } else if left_val != value {
                    if left_t2.map_or(true, |t2| t2 > new_time) {
                        self.points.insert(
                            left_t1,
                            StateDynamicsTimeLinePoint {
                                t1: left_t1,
                                t2: Some(new_time),
                                value: left_val,
                            },
                        );
                        self.points.insert(
                            new_time,
                            StateDynamicsTimeLinePoint {
                                t1: new_time,
                                t2: left_t2,
                                value,
                            },
                        );
                    } else if left_t2.map_or(true, |t2| t2 < new_time) {
                        self.points.insert(
                            left_t1,
                            StateDynamicsTimeLinePoint {
                                t1: left_t1,
                                t2: Some(new_time),
                                value: left_val,
                            },
                        );
                        self.points.insert(
                            new_time,
                            StateDynamicsTimeLinePoint {
                                t1: new_time,
                                t2: None,
                                value,
                            },
                        );
                    }
                }
            }

            // the new event falls on the left side of the existing timeline
            (None, Some((right_t1, right_t2, values_equal))) => {
                if values_equal {
                    self.points.remove(&right_t1);
                    self.points.insert(
                        new_time,
                        StateDynamicsTimeLinePoint {
                            t1: new_time,
                            t2: right_t2,
                            value,
                        },
                    );
                } else {
                    self.points.insert(
                        new_time,
                        StateDynamicsTimeLinePoint {
                            t1: new_time,
                            t2: Some(right_t1),
                            value,
                        },
                    );
                }
            }

            (None, None) => {
                self.points.insert(
                    new_time,
                    StateDynamicsTimeLinePoint {
                        t1: new_time,
                        t2: None,
                        value,
                    },
                );
            }
        }
    }
}

impl<T> Default for StateDynamicsTimeLine<T> {
    fn default() -> Self {
        StateDynamicsTimeLine {
            points: BTreeMap::new(),
        }
    }
}

impl StateDynamicsTimeLine<bool> {
    pub fn negate(&self) -> StateDynamicsTimeLine<bool> {
        self.map(|v| !v)
    }

    pub fn tl_duration_where(&self) -> EventTimeLine<u64> {
        let mut event_time_line = EventTimeLine::default();
        let mut duration = 0;

        // 1: false
        for point in &self.points {
            let start = point.1.t1;
            let end = point.1.t2;

            match end {
                Some(end) => {
                    if point.1.value {
                        let mut i = 0;
                        while (start + i) < end {
                            event_time_line.add_event_info(start + i, duration);
                            duration += 1;
                            i += 1;
                        }
                    } else {
                        let mut i = 0;
                        while (start + i) < end {
                            event_time_line.add_event_info(start + i, duration);
                            i += 1;
                        }
                    }
                }

                None => {
                    if point.1.value {
                        event_time_line.add_event_info(start, duration);
                    }
                }
            }
        }

        event_time_line
    }

    pub fn and(&self, that: StateDynamicsTimeLine<bool>) -> StateDynamicsTimeLine<bool> {
        self.zip_with(&that, |a, b| *a && *b)
    }

    pub fn or(&self, that: StateDynamicsTimeLine<bool>) -> StateDynamicsTimeLine<bool> {
        self.zip_with(&that, |a, b| *a || *b)
    }
}

impl<T: Debug + Clone + PartialOrd> StateDynamicsTimeLine<T> {
    // This turned out to be a mere conversion of events to state
    pub fn tl_latest_event_to_state(
        event_time_line: &EventTimeLine<T>,
    ) -> StateDynamicsTimeLine<T> {
        let mut state_dynamics_time_line = StateDynamicsTimeLine::default();

        for point in &event_time_line.points {
            state_dynamics_time_line.add_state_dynamic_info(point.t1, point.value.clone());
        }

        state_dynamics_time_line
    }

    // pub fn tl_has_existed_within(
    //     event_time_line: &EventTimeLine<T>,
    //     event_predicate: GolemEventPredicate<T>,
    //     seconds: u64,
    // ) -> StateDynamicsTimeLine<bool> {
    //     let mut state_dynamics_time_line = StateDynamicsTimeLine::default();
    //
    //     let mut previous_true_point: Option<u64> = None;
    //
    //     for event_time_line_point in &event_time_line.points {
    //         let is_predicate_true = event_predicate.evaluate(&event_time_line_point.value);
    //
    //         match previous_true_point {
    //             Some(t) if event_time_line_point.t1 > t + seconds => {
    //                 state_dynamics_time_line.add_state_dynamic_info(t + seconds, false);
    //                 previous_true_point = None;
    //             }
    //             _ => {}
    //         }
    //
    //         if is_predicate_true {
    //             state_dynamics_time_line.add_state_dynamic_info(event_time_line_point.t1, true);
    //             previous_true_point = Some(event_time_line_point.t1);
    //         } else {
    //             state_dynamics_time_line.add_state_dynamic_info(event_time_line_point.t1, false);
    //             previous_true_point = None;
    //         }
    //     }
    //
    //     // If the last known value is a true, then add an extra data point that expires at t + seconds
    //     match previous_true_point {
    //         Some(t) => {
    //             state_dynamics_time_line.add_state_dynamic_info(t + seconds, false);
    //         }
    //         None => {}
    //     }
    //
    //     state_dynamics_time_line
    // }

    pub fn beginning(&self) -> Option<u64> {
        self.points.first_key_value().map(|(k, _)| *k)
    }

    pub fn equal_to(&self, constant: T) -> StateDynamicsTimeLine<bool> {
        self.map(|v| *v == constant)
    }

    pub fn greater_than(&self, constant: T) -> StateDynamicsTimeLine<bool> {
        self.map(|v| *v > constant)
    }

    pub fn greater_than_or_equal_to(&self, constant: T) -> StateDynamicsTimeLine<bool> {
        self.map(|v| *v >= constant)
    }

    pub fn less_than(&self, constant: T) -> StateDynamicsTimeLine<bool> {
        self.map(|v| *v < constant)
    }

    pub fn less_than_or_equal_to(&self, constant: T) -> StateDynamicsTimeLine<bool> {
        self.map(|v| *v <= constant)
    }

    pub fn zip_with<F>(&self, other: &StateDynamicsTimeLine<T>, f: F) -> StateDynamicsTimeLine<T>
    where
        F: Fn(&T, &T) -> T,
    {
        let mut flattened_time_line_points: BTreeMap<u64, StateDynamicsTimeLinePoint<T>> =
            BTreeMap::new();

        let AlignedStateDynamicsTimeLine {
            time_line1: aligned_left,
            time_line2: aligned_right,
            removed_points_timeline1,
            removed_points_timeline2,
        } = AlignedStateDynamicsTimeLine::from_left_and_right(self.clone(), other.clone());

        if let Some(removed) = removed_points_timeline1 {
            for (k, point) in removed.points {
                flattened_time_line_points.insert(k, point);
            }
        }

        if let Some(removed) = removed_points_timeline2 {
            for (k, point) in removed.points {
                flattened_time_line_points.insert(k, point);
            }
        }

        let mut self_iter = aligned_left.points.iter().peekable();
        let mut other_iter = aligned_right.points.iter().peekable();

        loop {
            let self_point = match self_iter.next() {
                Some(p) => p,
                None => break,
            };
            let other_point = match other_iter.next() {
                Some(p) => p,
                None => break,
            };

            let Boundaries {
                left: left_ex,
                intersection,
                right: right_ex,
            } = Boundaries::get_boundaries(self_point.1, other_point.1);

            flattened_time_line_points.insert(intersection.t1, intersection.apply_f(&f));

            if let Some(left_ex) = left_ex {
                flattened_time_line_points
                    .entry(left_ex.t1)
                    .and_modify(|existing| {
                        *existing = StateDynamicsTimeLinePoint {
                            t1: left_ex.t1,
                            t2: left_ex.t2,
                            value: ZipResult::Both((
                                &existing.value,
                                Box::new(left_ex.value.clone()),
                            ))
                            .merge(&f),
                        }
                    })
                    .or_insert(left_ex.apply_f(&f));
            }

            if let Some(right_ex) = right_ex {
                flattened_time_line_points
                    .entry(right_ex.t1)
                    .and_modify(|existing| {
                        *existing = StateDynamicsTimeLinePoint {
                            t1: right_ex.t1,
                            t2: right_ex.t2,
                            value: ZipResult::Both((
                                &existing.value,
                                Box::new(right_ex.value.clone()),
                            ))
                            .merge(&f),
                        }
                    })
                    .or_insert(right_ex.apply_f(&f));
            }
        }

        StateDynamicsTimeLine {
            points: flattened_time_line_points,
        }
    }
}

// ~~ represents `forever`
// -- denotes a finite boundary
#[cfg(test)]
mod tests {
    use crate::event_timeline::{EventTimeLine, EventTimeLinePoint};
    use crate::state_dynamic_timeline::StateDynamicsTimeLine;
    use crate::state_dynamic_timeline_point::StateDynamicsTimeLinePoint;
    use std::collections::BTreeMap;

    #[test]
    fn test_equal_to() {
        let mut timeline: StateDynamicsTimeLine<String> = StateDynamicsTimeLine::default();
        timeline.add_state_dynamic_info(5, "play".to_string());
        timeline.add_state_dynamic_info(8, "pause".to_string());

        let is_playing_timeline = timeline.equal_to("play".to_string());

        let expected_points = BTreeMap::from([
            (
                5,
                StateDynamicsTimeLinePoint {
                    t1: 5,
                    t2: Some(8),
                    value: true,
                },
            ),
            (
                8,
                StateDynamicsTimeLinePoint {
                    t1: 8,
                    t2: None,
                    value: false,
                },
            ),
        ]);

        let expected = StateDynamicsTimeLine {
            points: expected_points,
        };

        assert_eq!(is_playing_timeline, expected)
    }

    // t1~~~~(playing)~~~~~~~~~~~~>
    //       t2~~~~(movie)~~~~~~~~~~>
    // Expected Result:
    //   t1 - t2     : playing
    //   t2 -> future: playing a movie
    #[test]
    fn test_zip_with_scenario1() {
        let mut timeline1: StateDynamicsTimeLine<String> = StateDynamicsTimeLine::default();
        timeline1.add_state_dynamic_info(5, "playing".to_string());

        let mut timeline2 = StateDynamicsTimeLine::default();
        timeline2.add_state_dynamic_info(7, "movie".to_string());

        let result1 = timeline1.zip_with(&timeline2, |a, b| format!("{} {}", a, b));

        let result2 = timeline2.zip_with(&timeline1, |a, b| format!("{} {}", a, b));

        let mut btree_map1 = BTreeMap::new();
        btree_map1.insert(
            5,
            StateDynamicsTimeLinePoint {
                t1: 5,
                t2: Some(7),
                value: "playing".to_string(),
            },
        );

        btree_map1.insert(
            7,
            StateDynamicsTimeLinePoint {
                t1: 7,
                t2: None,
                value: "playing movie".to_string(),
            },
        );

        let expected1 = StateDynamicsTimeLine { points: btree_map1 };

        let mut btree_map2 = BTreeMap::new();

        btree_map2.insert(
            5,
            StateDynamicsTimeLinePoint {
                t1: 5,
                t2: Some(7),
                value: "playing".to_string(),
            },
        );

        btree_map2.insert(
            7,
            StateDynamicsTimeLinePoint {
                t1: 7,
                t2: None,
                value: "movie playing".to_string(),
            },
        );

        let expected2 = StateDynamicsTimeLine { points: btree_map2 };

        assert_eq!(result1, expected1);
        assert_eq!(result2, expected2);
    }

    // t1-------(playing)---t3~~~~(pause)~~~~~>
    //      t2----(movie)---------t4~~~~(cartoon)~~~~~>
    // Expected Result:
    //   t1 - t2    : playing
    //   t2 - t3    : playing a movie
    //   t3 - t4    : paused movie
    //   t4 - future: paused cartoon
    #[test]
    fn test_zip_with_scenario2() {
        let mut timeline1 = StateDynamicsTimeLine::default();
        timeline1.add_state_dynamic_info(5, "playing".to_string());
        timeline1.add_state_dynamic_info(8, "pause".to_string());

        let mut timeline2 = StateDynamicsTimeLine::default();
        timeline2.add_state_dynamic_info(7, "movie".to_string());
        timeline2.add_state_dynamic_info(9, "cartoon".to_string());

        let result = timeline2.zip_with(&timeline1, |a, b| format!("{} {}", a, b));

        let mut btree_map = BTreeMap::new();

        btree_map.insert(
            5,
            StateDynamicsTimeLinePoint {
                t1: 5,
                t2: Some(7),
                value: "playing".to_string(),
            },
        );

        btree_map.insert(
            7,
            StateDynamicsTimeLinePoint {
                t1: 7,
                t2: Some(8),
                value: "movie playing".to_string(),
            },
        );

        btree_map.insert(
            8,
            StateDynamicsTimeLinePoint {
                t1: 8,
                t2: Some(9),
                value: "movie pause".to_string(),
            },
        );

        btree_map.insert(
            9,
            StateDynamicsTimeLinePoint {
                t1: 9,
                t2: None,
                value: "cartoon pause".to_string(),
            },
        );

        let expected = StateDynamicsTimeLine { points: btree_map };

        assert_eq!(result, expected);
    }

    // t1-------(playing)----------t4~~~~(pause)~~~~~>
    //      t2----(movie)---t3~~~~(cartoon)~~~~~>
    // Expected Result:
    //   t1 - t2    : playing
    //   t2 - t3    : playing a movie
    //   t3 - t4    : playing a cartoon
    //   t4 - future: paused cartoon
    #[test]
    fn test_zip_with_scenario3() {
        let mut timeline1 = StateDynamicsTimeLine::default();
        timeline1.add_state_dynamic_info(1, "playing".to_string());
        timeline1.add_state_dynamic_info(4, "pause".to_string());

        let mut timeline2 = StateDynamicsTimeLine::default();
        timeline2.add_state_dynamic_info(2, "movie".to_string());
        timeline2.add_state_dynamic_info(3, "cartoon".to_string());

        let result = timeline2.zip_with(&timeline1, |a, b| format!("{} {}", a, b));

        let mut btree_map = BTreeMap::new();

        btree_map.insert(
            1,
            StateDynamicsTimeLinePoint {
                t1: 1,
                t2: Some(2),
                value: "playing".to_string(),
            },
        );

        btree_map.insert(
            2,
            StateDynamicsTimeLinePoint {
                t1: 2,
                t2: Some(3),
                value: "movie playing".to_string(),
            },
        );

        btree_map.insert(
            3,
            StateDynamicsTimeLinePoint {
                t1: 3,
                t2: Some(4),
                value: "cartoon playing".to_string(),
            },
        );

        btree_map.insert(
            4,
            StateDynamicsTimeLinePoint {
                t1: 4,
                t2: None,
                value: "cartoon pause".to_string(),
            },
        );

        let expected = StateDynamicsTimeLine { points: btree_map };

        assert_eq!(result, expected);
    }

    // t1-----(pause)------t2~~~~~(playing)~~~~~>
    //                         t3~~~~~(movie)~~~~>
    // Expected Result:
    //   t1 - t2    : pause
    //   t2 - t3    : playing
    //   t2 - future: playing a movie
    #[test]
    fn test_zip_with_scenario4() {
        let mut timeline1 = StateDynamicsTimeLine::default();
        timeline1.add_state_dynamic_info(1, "pause".to_string());
        timeline1.add_state_dynamic_info(2, "playing".to_string());

        let mut timeline2 = StateDynamicsTimeLine::default();
        timeline2.add_state_dynamic_info(3, "movie".to_string());

        let result = timeline1.zip_with(&timeline2, |a, b| format!("{} {}", a, b));

        let mut btree_map = BTreeMap::new();

        btree_map.insert(
            1,
            StateDynamicsTimeLinePoint {
                t1: 1,
                t2: Some(2),
                value: "pause".to_string(),
            },
        );

        btree_map.insert(
            2,
            StateDynamicsTimeLinePoint {
                t1: 2,
                t2: Some(3),
                value: "playing".to_string(),
            },
        );

        btree_map.insert(
            3,
            StateDynamicsTimeLinePoint {
                t1: 3,
                t2: None,
                value: "playing movie".to_string(),
            },
        );

        let expected = StateDynamicsTimeLine { points: btree_map };

        assert_eq!(result, expected);
    }

    // Input t1------(pause)-----t2~~~~~(playing)~~~~~>
    // Expiration time: 1 seconds, predicate: if playing
    // t1------(false)-----t2----(true)-------t3~~~(false)~~~~>
    // #[test]
    // fn test_tl_has_existed_within_scenario1() {
    //     let mut event_time_line = EventTimeLine::default();
    //     event_time_line.add_event_info(1, "pause".to_string());
    //     event_time_line.add_event_info(2, "playing".to_string());
    //
    //     let event_predicate = event_predicate::col("event").equal_to::<String>(string("playing"));
    //
    //     let result =
    //         StateDynamicsTimeLine::tl_has_existed_within(&event_time_line, event_predicate, 1);
    //
    //     let mut btree_map = BTreeMap::new();
    //
    //     btree_map.insert(1, StateDynamicsTimeLinePoint {
    //         t1: 1,
    //         t2: Some(2),
    //         value: false,
    //     });
    //
    //     btree_map.insert(2, StateDynamicsTimeLinePoint {
    //         t1: 2,
    //         t2: Some(3),
    //         value: true,
    //     });
    //
    //     btree_map.insert(3, StateDynamicsTimeLinePoint {
    //         t1: 3,
    //         t2: None,
    //         value: false,
    //     });
    //
    //     let expected = StateDynamicsTimeLine {
    //         points: btree_map,
    //     };
    //
    //
    //     assert_eq!(result, expected);
    // }

    // Input t1------(pause)-----t2----------(playing)------t5~~~~~~(playing)~~~~>
    // Expiration time: 2 seconds, predicate: if playing
    // t1--------(false)-----t2----(true)---t4----(false)---t5----(true)---t7~~~~(false)~~~~~~>
    // #[test]
    // fn test_tl_has_existed_within_scenario2() {
    //     let mut event_time_line = EventTimeLine::default();
    //     event_time_line.add_event_info(1, "pause".to_string());
    //     event_time_line.add_event_info(2, "playing".to_string());
    //     event_time_line.add_event_info(5, "playing".to_string());
    //
    //     let event_predicate = event_predicate::col("event").equal_to::<String>(string("playing"));
    //
    //     let result =
    //         StateDynamicsTimeLine::tl_has_existed_within(&event_time_line, event_predicate, 2);
    //
    //     let mut btree_map = BTreeMap::new();
    //
    //     btree_map.insert(1, StateDynamicsTimeLinePoint {
    //         t1: 1,
    //         t2: Some(2),
    //         value: false,
    //     });
    //
    //     btree_map.insert(2, StateDynamicsTimeLinePoint {
    //         t1: 2,
    //         t2: Some(4),
    //         value: true,
    //     });
    //
    //
    //     btree_map.insert(4, StateDynamicsTimeLinePoint {
    //         t1: 4,
    //         t2: Some(5),
    //         value: false,
    //     });
    //
    //     btree_map.insert(5, StateDynamicsTimeLinePoint {
    //         t1: 5,
    //         t2: Some(7),
    //         value: true,
    //     });
    //
    //     btree_map.insert(7, StateDynamicsTimeLinePoint {
    //         t1: 7,
    //         t2: None,
    //         value: false,
    //     });
    //
    //     let expected = StateDynamicsTimeLine {
    //         points: btree_map,
    //     };
    //
    //
    //     assert_eq!(result, expected);
    // }

    #[test]
    fn test_negatable() {
        let mut timeline = StateDynamicsTimeLine::default();
        timeline.add_state_dynamic_info(1, true);
        timeline.add_state_dynamic_info(2, false);
        timeline.add_state_dynamic_info(3, true);

        let result = timeline.negate();

        let mut btree_map = BTreeMap::new();

        btree_map.insert(
            1,
            StateDynamicsTimeLinePoint {
                t1: 1,
                t2: Some(2),
                value: false,
            },
        );

        btree_map.insert(
            2,
            StateDynamicsTimeLinePoint {
                t1: 2,
                t2: Some(3),
                value: true,
            },
        );

        btree_map.insert(
            3,
            StateDynamicsTimeLinePoint {
                t1: 3,
                t2: None,
                value: false,
            },
        );

        let expected = StateDynamicsTimeLine { points: btree_map };

        assert_eq!(result, expected);
    }

    #[test]
    fn tl_duration_where() {
        let mut timeline = StateDynamicsTimeLine::default();
        timeline.add_state_dynamic_info(1, true);
        timeline.add_state_dynamic_info(3, false);
        timeline.add_state_dynamic_info(5, true);
        timeline.add_state_dynamic_info(7, true);

        dbg!(timeline.clone());
        let result = timeline.tl_duration_where();

        let expected = EventTimeLine {
            points: vec![
                EventTimeLinePoint { t1: 1, value: 0 },
                EventTimeLinePoint { t1: 2, value: 1 },
                EventTimeLinePoint { t1: 3, value: 2 },
                EventTimeLinePoint { t1: 4, value: 2 },
                EventTimeLinePoint { t1: 5, value: 2 },
            ],
        };

        assert_eq!(result, expected);
    }
}
