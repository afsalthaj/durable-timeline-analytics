// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::fmt::Debug;

use crate::internals::zip_result::{Side, ZipResult};
use crate::state_dynamic_timeline_point::StateDynamicsTimeLinePoint;

pub struct Boundaries<'t, T: Clone> {
    pub left: Option<StateDynamicsTimeLinePoint<ZipResult<'t, T>>>,
    pub intersection: StateDynamicsTimeLinePoint<ZipResult<'t, T>>,
    pub right: Option<StateDynamicsTimeLinePoint<ZipResult<'t, T>>>,
}

impl<'t, T: Debug + Clone> Boundaries<'t, T> {
    pub fn get_boundaries(
        left: &'t StateDynamicsTimeLinePoint<T>,
        right: &'t StateDynamicsTimeLinePoint<T>,
    ) -> Boundaries<'t, T> {
        let intersection_boundary_left = left.t1.max(right.t1);

        let intersection_boundary_right = match (left.t2, right.t2) {
            (Some(t2), None) => Some(t2),
            (None, Some(t2)) => Some(t2),
            (Some(t2), Some(t2_other)) => Some(t2.min(t2_other)),
            (None, None) => None,
        };

        let intersection = StateDynamicsTimeLinePoint {
            t1: intersection_boundary_left,
            t2: intersection_boundary_right,
            value: ZipResult::Both((
                &left.value,
                Box::new(ZipResult::Singleton(&right.value, Side::Right)),
            )),
        };

        // left boundary optional: t1 -> t2
        let left_boundary = if left.t1 == right.t1 {
            None
        } else {
            let left_boundary_left = left.t1.min(right.t1);
            let left_boundary_right = intersection_boundary_left;
            Some(if left_boundary_left == left.t1 {
                StateDynamicsTimeLinePoint {
                    t1: left_boundary_left,
                    t2: Some(left_boundary_right),
                    value: ZipResult::Singleton(&left.value, Side::Left),
                }
            } else {
                // if t1x0 == other_point.t1, then it means t1 is before t2 and the value exists only in self time line.
                StateDynamicsTimeLinePoint {
                    t1: left_boundary_left,
                    t2: Some(left_boundary_right),
                    value: ZipResult::Singleton(&right.value, Side::Right),
                }
            })
        };

        // right boundary optional
        let right_boundary = if left.t2 == right.t2 {
            None
        } else {
            let right_boundary_left = intersection_boundary_right;
            let right_boundary_right = match (left.t2, right.t2) {
                (Some(t2), None) => Some(t2),
                (None, Some(t2)) => Some(t2),
                (Some(t2), Some(t2_other)) => Some(t2.max(t2_other)),
                (None, None) => None,
            };
            match right_boundary_left {
                Some(t2x0) => {
                    if Some(t2x0) == left.t2 {
                        Some(StateDynamicsTimeLinePoint {
                            t1: t2x0,
                            t2: right_boundary_right,
                            value: ZipResult::Singleton(&right.value, Side::Right),
                        })
                    } else {
                        Some(StateDynamicsTimeLinePoint {
                            t1: t2x0,
                            t2: right_boundary_right,
                            value: ZipResult::Singleton(&left.value, Side::Left),
                        })
                    }
                }
                None => None,
            }
        };

        Boundaries {
            left: left_boundary,
            intersection,
            right: right_boundary,
        }
    }
}
