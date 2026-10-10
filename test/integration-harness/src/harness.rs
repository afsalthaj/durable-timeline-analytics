// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Test harness orchestration.
//!
//! Checks that infrastructure (Golem, Kafka) is reachable and provides helpers
//! for test setup. The Golem server is started and stopped by the Makefile;
//! Kafka is started here via Docker Compose when it isn't already running.

use std::error::Error;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common_lib::SessionId;
use timeline_client::{GolemCli, TimelineClient};

pub type TestResult = Result<(), Box<dyn Error>>;

const GOLEM_PORT: u16 = 9881;
const KAFKA_PORT: u16 = 9092;

/// The workspace root, which is also the Golem application directory.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(2)
        .unwrap_or(manifest_dir)
        .to_path_buf()
}

fn is_listening(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_secs(2)).is_ok()
}

/// Verify the Golem server is running.
///
/// The Makefile (`cargo make smoke`, etc.) starts and stops the server.
pub fn ensure_golem_running() -> Result<(), Box<dyn Error>> {
    if is_listening(GOLEM_PORT) {
        Ok(())
    } else {
        Err("Golem server not running. Use `cargo make smoke` (or `cargo make integration`), which starts and stops the server.".into())
    }
}

/// A client for the timeline agents of the workspace's Golem application.
pub fn client() -> TimelineClient {
    TimelineClient::new(GolemCli::new(workspace_root()))
}

/// Checks the server is up and deploys the component, once per test binary.
pub fn setup() -> Result<TimelineClient, Box<dyn Error>> {
    static DEPLOYED: OnceLock<Result<(), String>> = OnceLock::new();
    ensure_golem_running()?;
    let client = client();
    DEPLOYED
        .get_or_init(|| client.cli().deploy().map_err(|e| e.to_string()))
        .clone()?;
    Ok(client)
}

/// A session ID unique to this test run, so reruns against the same server
/// don't hit agents left over from earlier runs.
pub fn unique_session(prefix: &str) -> SessionId {
    static RUN_ID: OnceLock<u128> = OnceLock::new();
    let run_id = RUN_ID.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default()
    });
    SessionId::new(format!("{prefix}-{run_id}"))
}

/// Ensure Kafka is reachable at localhost:9092, starting it via Docker Compose if needed.
pub fn ensure_kafka_running() -> Result<(), Box<dyn Error>> {
    if is_listening(KAFKA_PORT) {
        return Ok(());
    }

    let status = Command::new("docker")
        .args([
            "compose",
            "-f",
            "test/integration-harness/docker/compose.yml",
            "up",
            "-d",
        ])
        .current_dir(workspace_root())
        .status()?;
    if !status.success() {
        return Err("Failed to start Kafka via docker compose".into());
    }

    for _ in 0..30 {
        std::thread::sleep(Duration::from_secs(1));
        if is_listening(KAFKA_PORT) {
            return Ok(());
        }
    }
    Err("Timeout waiting for Kafka at localhost:9092".into())
}
