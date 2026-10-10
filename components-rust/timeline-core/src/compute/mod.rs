// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Timeline computations performed by the agents, free of any Golem RPC so
//! they can be unit-tested natively.
mod aggregate;
mod derived;
mod leaf;

pub use aggregate::{AggregatorLink, ReportRoute, SessionAggregates};
pub use derived::{ChildUpdate, DerivedState};
pub use leaf::{LeafChange, LeafState};

use common_lib::{EventValue, StateDynamicsTimeLinePoint, StatePoint};

fn to_state_point<T>(
    point: Option<&StateDynamicsTimeLinePoint<T>>,
    to_value: impl Fn(&T) -> EventValue,
) -> Option<StatePoint> {
    point.map(|p| StatePoint {
        t1: p.t1,
        t2: p.t2,
        value: to_value(&p.value),
    })
}
