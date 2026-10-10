// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Native client for the timeline agents. Invocations go through the `golem`
//! CLI, which handles authentication, routing and value encoding.
pub mod cli;
pub mod rust_syntax;
pub mod schema_json;
pub mod timeline;

pub use cli::{AgentRef, AgentType, ClientError, GolemCli};
pub use timeline::TimelineClient;
