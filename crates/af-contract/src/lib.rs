//! Shared Node Contract for AxisFlow.
//!
//! Every `af-*` node implements this uniform interface:
//!   - `--describe`  -> prints the capability manifest as JSON
//!   - `--version`   -> prints the semver string
//!   - (default)     -> reads input JSON on stdin, writes output JSON on stdout
//!
//! Exit codes are standardized so the Orchestrator can decide retry/continue.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability manifest emitted by `af-* --describe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    /// JSON Schema (draft 2020-12) describing the node's input object.
    #[serde(default = "Value::default")]
    pub inputs: Value,
    /// JSON Schema describing the node's output object.
    #[serde(default = "Value::default")]
    pub outputs: Value,
    /// Names of input fields that are credentials (resolved via `af-vault`).
    #[serde(default)]
    pub secrets: Vec<String>,
    /// If true, stdout is NDJSON (one JSON object per line).
    #[serde(default)]
    pub streaming: bool,
    /// Hint: safe to retry on transient failure.
    #[serde(default)]
    pub idempotent: bool,
}

/// Standardized process exit codes.
pub mod exit_code {
    pub const SUCCESS: i32 = 0;
    pub const GENERIC: i32 = 1;
    pub const RETRYABLE: i32 = 2;
    pub const AUTH: i32 = 3;
    pub const INVALID_INPUT: i32 = 4;
}

/// Print a manifest as pretty JSON to stdout (used by `--describe`).
pub fn print_manifest(m: &Manifest) {
    println!(
        "{}",
        serde_json::to_string_pretty(m).expect("manifest serialize")
    );
}

/// Write a structured error to stderr and exit with the given code.
pub fn fail(code: i32, message: impl AsRef<str>) -> ! {
    let payload = serde_json::json!({
        "error": message.as_ref(),
        "code": code,
    });
    eprintln!("{}", payload);
    std::process::exit(code);
}

/// Convenience: read the entire stdin as a JSON `Value`.
pub fn read_input() -> Value {
    use std::io::Read;
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        fail(exit_code::INVALID_INPUT, "failed to read stdin");
    }
    match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(e) => fail(exit_code::INVALID_INPUT, format!("invalid input JSON: {e}")),
    }
}
