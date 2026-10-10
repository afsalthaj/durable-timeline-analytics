// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Invokes agents through `golem agent invoke --format json`.
use std::fmt::{self, Display};
use std::path::PathBuf;
use std::process::Command;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use common_lib::AgentName;

use crate::rust_syntax::{to_rust_syntax, RenderError};
use crate::schema_json::{DecodeError, TypedResult};

#[derive(Debug)]
pub enum ClientError {
    Render(RenderError),
    Spawn(std::io::Error),
    CommandFailed(CommandFailure),
    MissingResult { stdout: String },
    Json(serde_json::Error),
    Decode(DecodeError),
    Agent(common_lib::TimelineError),
}

/// Output of a `golem` invocation that exited unsuccessfully.
#[derive(Debug)]
pub struct CommandFailure {
    pub stdout: String,
    pub stderr: String,
}

impl Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Render(e) => write!(f, "{e}"),
            ClientError::Spawn(e) => write!(f, "cannot run the golem CLI: {e}"),
            ClientError::CommandFailed(failure) => write!(
                f,
                "golem CLI failed\nstdout: {}\nstderr: {}",
                failure.stdout, failure.stderr
            ),
            ClientError::MissingResult { stdout } => {
                write!(
                    f,
                    "golem CLI printed no invocation result\nstdout: {stdout}"
                )
            }
            ClientError::Json(e) => write!(f, "invalid JSON from golem CLI: {e}"),
            ClientError::Decode(e) => write!(f, "{e}"),
            ClientError::Agent(e) => write!(f, "agent returned an error: {e}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<RenderError> for ClientError {
    fn from(e: RenderError) -> Self {
        ClientError::Render(e)
    }
}

impl From<DecodeError> for ClientError {
    fn from(e: DecodeError) -> Self {
        ClientError::Decode(e)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> Self {
        ClientError::Json(e)
    }
}

impl From<common_lib::TimelineError> for ClientError {
    fn from(e: common_lib::TimelineError) -> Self {
        ClientError::Agent(e)
    }
}

/// The agent types exported by the `timeline:core` component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    TimelineDriver,
    EventProcessor,
    TimelineProcessor,
    Aggregator,
}

impl AgentType {
    fn as_str(self) -> &'static str {
        match self {
            AgentType::TimelineDriver => "TimelineDriver",
            AgentType::EventProcessor => "EventProcessor",
            AgentType::TimelineProcessor => "TimelineProcessor",
            AgentType::Aggregator => "Aggregator",
        }
    }
}

/// One agent instance, addressed as `AgentType("name")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRef {
    pub agent_type: AgentType,
    pub name: AgentName,
}

impl AgentRef {
    pub fn new(agent_type: AgentType, name: AgentName) -> Self {
        AgentRef { agent_type, name }
    }

    /// The agent id as accepted by `golem agent invoke`, e.g. `EventProcessor("s-has-existed-2")`.
    pub fn to_agent_id(&self) -> Result<String, RenderError> {
        Ok(format!(
            "{}({})",
            self.agent_type.as_str(),
            to_rust_syntax(self.name.as_str())?
        ))
    }
}

/// Renders invocation arguments.
pub fn arg<T: Serialize + ?Sized>(value: &T) -> Result<String, RenderError> {
    to_rust_syntax(value)
}

/// Runs the `golem` CLI against the application in `app_dir`.
#[derive(Debug, Clone)]
pub struct GolemCli {
    binary: PathBuf,
    app_dir: PathBuf,
}

impl GolemCli {
    /// Uses the `golem` binary from `$GOLEM_CLI`, or `golem` on `PATH`.
    pub fn new(app_dir: impl Into<PathBuf>) -> Self {
        let binary = std::env::var_os("GOLEM_CLI")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("golem"));
        GolemCli {
            binary,
            app_dir: app_dir.into(),
        }
    }

    pub fn with_binary(mut self, binary: impl Into<PathBuf>) -> Self {
        self.binary = binary.into();
        self
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.current_dir(&self.app_dir);
        command
    }

    fn run(&self, mut command: Command) -> Result<String, ClientError> {
        let output = command.output().map_err(ClientError::Spawn)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.success() {
            Ok(stdout)
        } else {
            Err(ClientError::CommandFailed(CommandFailure {
                stdout,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }))
        }
    }

    /// Runs `golem deploy --yes`.
    pub fn deploy(&self) -> Result<(), ClientError> {
        let mut command = self.command();
        command.args(["deploy", "--yes"]);
        self.run(command).map(|_| ())
    }

    /// Invokes `method` and waits for its result.
    pub fn invoke<T: DeserializeOwned>(
        &self,
        agent: &AgentRef,
        method: &str,
        args: &[String],
    ) -> Result<T, ClientError> {
        let mut command = self.command();
        command
            .args(["--format", "json", "agent", "invoke", "--yes"])
            .arg(agent.to_agent_id()?)
            .arg(method)
            .args(args);
        let stdout = self.run(command)?;
        let named = invocation_result(&stdout)?;
        Ok(serde_json::from_value(named)?)
    }

    /// Enqueues `method` without waiting for it to run.
    pub fn trigger(
        &self,
        agent: &AgentRef,
        method: &str,
        args: &[String],
    ) -> Result<(), ClientError> {
        let mut command = self.command();
        command
            .args(["--format", "json", "agent", "invoke", "--yes", "--trigger"])
            .arg(agent.to_agent_id()?)
            .arg(method)
            .args(args);
        self.run(command).map(|_| ())
    }
}

/// Extracts the named result from the JSON documents printed by `golem agent invoke`.
/// Functions without a return value yield `null`.
pub fn invocation_result(stdout: &str) -> Result<Value, ClientError> {
    let document = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|doc| doc.get("$type").and_then(Value::as_str) == Some("agent.invoke"))
        .ok_or_else(|| ClientError::MissingResult {
            stdout: stdout.to_string(),
        })?;
    match document.get("resultJson") {
        None | Some(Value::Null) => Ok(Value::Null),
        Some(result) => {
            let typed: TypedResult = serde_json::from_value(result.clone())?;
            Ok(typed.to_named_json()?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn agent_id_quotes_the_name() -> TestResult {
        let agent = AgentRef::new(AgentType::EventProcessor, AgentName::new("s-has-existed-2"));
        assert_eq!(agent.to_agent_id()?, r#"EventProcessor("s-has-existed-2")"#);
        Ok(())
    }

    #[test]
    fn finds_the_invoke_document_among_stream_events() -> TestResult {
        let stdout = concat!(
            r#"{"$type":"agent.stream","kind":"invocation-started"}"#,
            "\n",
            r#"{"$type":"agent.invoke","result":"42","resultJson":{"graph":{"defs":[],"root":{"kind":"u64","value":{}}},"value":{"kind":"u64","value":42}}}"#,
            "\n"
        );
        assert_eq!(invocation_result(stdout)?, Value::from(42u64));
        Ok(())
    }

    #[test]
    fn unit_results_are_null() -> TestResult {
        let stdout = r#"{"$type":"agent.invoke","result":"()"}"#;
        assert_eq!(invocation_result(stdout)?, Value::Null);
        Ok(())
    }

    #[test]
    fn missing_invoke_document_is_an_error() {
        let stdout = r#"{"$type":"agent.stream","kind":"stream-closed"}"#;
        assert!(matches!(
            invocation_result(stdout),
            Err(ClientError::MissingResult { .. })
        ));
    }
}
