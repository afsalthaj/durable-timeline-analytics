// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::fmt::Debug;

use crate::state_dynamic_timeline::StateDynamicsTimeLine;

// Aligning two timelines is an important step before you start zipping timelines
// In real world, if the align returns empty timelines probably it's a good idea to wait for the
// stream to get some data in each worker to get a fuller timeline

// Input: Two misaligned timelines
// Timeline1: t1------------t2-----------t4
// Timeline2:                    t3--------------t5
// Output: Aligned timeline returns (t2 - t4) and (t3 - t5)

// Input:  Two misaligned timelines
// TimeLine1:                  t3-----------t5
// TimeLine2: t1----------t2---------t4
// Output: AlignedTimeLine returns (t3 - t5) and (t2 -t4)
pub struct AlignedStateDynamicsTimeLine<T> {
    pub time_line1: StateDynamicsTimeLine<T>,
    pub time_line2: StateDynamicsTimeLine<T>,
    pub removed_points_timeline1: Option<StateDynamicsTimeLine<T>>,
    pub removed_points_timeline2: Option<StateDynamicsTimeLine<T>>,
}

impl<T: Clone + Debug + PartialEq + PartialOrd> AlignedStateDynamicsTimeLine<T> {
    pub fn from_left_and_right(
        mut left: StateDynamicsTimeLine<T>,
        right: StateDynamicsTimeLine<T>,
    ) -> AlignedStateDynamicsTimeLine<T> {
        if left.beginning() <= right.beginning() {
            let boundary = right.beginning().and_then(|t| left.boundary(t));

            match boundary {
                Some(b) => {
                    let aligned_points = left.points.split_off(&b);
                    AlignedStateDynamicsTimeLine {
                        time_line1: StateDynamicsTimeLine {
                            points: aligned_points,
                        },
                        time_line2: right,
                        removed_points_timeline1: Some(left),
                        removed_points_timeline2: None,
                    }
                }
                None => {
                    let removed = left.clone();
                    AlignedStateDynamicsTimeLine {
                        time_line1: left,
                        time_line2: right,
                        removed_points_timeline1: Some(removed),
                        removed_points_timeline2: None,
                    }
                }
            }
        } else {
            let mut right = right;
            let boundary = left.beginning().and_then(|t| right.boundary(t));

            match boundary {
                Some(b) => {
                    let aligned_points = right.points.split_off(&b);
                    AlignedStateDynamicsTimeLine {
                        time_line1: left,
                        time_line2: StateDynamicsTimeLine {
                            points: aligned_points,
                        },
                        removed_points_timeline1: None,
                        removed_points_timeline2: Some(right),
                    }
                }
                None => {
                    let removed = right.clone();
                    AlignedStateDynamicsTimeLine {
                        time_line1: left,
                        time_line2: right,
                        removed_points_timeline1: None,
                        removed_points_timeline2: Some(removed),
                    }
                }
            }
        }
    }
}
