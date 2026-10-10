// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};

use crate::event::EventColumnName;

/// A cross-session aggregation function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "golem",
    derive(
        golem_rust::Schema,
        golem_rust::IntoWire,
        golem_rust::FromWire,
        golem_rust::WireSchema
    )
)]
pub enum AggregationFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

impl Display for AggregationFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AggregationFunction::Count => "count",
            AggregationFunction::Sum => "sum",
            AggregationFunction::Avg => "avg",
            AggregationFunction::Min => "min",
            AggregationFunction::Max => "max",
        })
    }
}

/// Aggregate a timeline's root value across sessions, grouped by the value of
/// an event column (e.g. `cdn`).
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
pub struct AggregationConfig {
    pub group_by_column: EventColumnName,
    pub functions: Vec<AggregationFunction>,
}
