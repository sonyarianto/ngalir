//! AxisFlow Orchestrator (v1).
//!
//! Reads a Flow Spec (YAML/JSON), discovers node binaries, validates inputs
//! against node manifests (JSON Schema), builds a DAG from `inputs`/`when`
//! wiring, then executes nodes as `af-<use>` subprocesses with bounded
//! concurrency. Inter-node data is piped as JSON via stdin/stdout.

use af_contract::Manifest;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};
use tracing::{error, info, info_span, warn, Instrument};
use tracing_subscriber::fmt::format::FmtSpan;

// ── Flow Spec types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct FlowSpec {
    version: u32,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    nodes: Vec<NodeSpec>,
}

fn default_concurrency() -> usize {
    8
}

#[derive(Debug, Clone, Deserialize)]
struct NodeSpec {
    id: String,
    #[serde(rename = "use")]
    use_: String,
    #[serde(default)]
    with: Value,
    #[serde(default)]
    inputs: HashMap<String, String>,
    #[serde(default)]
    when: Option<String>,
    #[serde(default)]
    on_error: Option<String>,
}

// ── Node registry (binary discovery + manifest) ────────────────────────────

#[derive(Debug, Clone)]
struct NodeBin {
    binary: String,
    manifest: Manifest,
}

/// Discover every unique `use_:` node type referenced in the flow.
/// Runs `af-<use> --describe`, parses the manifest, and runs
/// structural pre-flight checks (binary exists, required inputs present,
/// manifest schema itself is valid).
async fn preflight(flow: &FlowSpec) -> Result<HashMap<String, NodeBin>> {
    let mut cache = HashMap::new();
    for node in &flow.nodes {
        if cache.contains_key(&node.use_) {
            continue;
        }
        let bin = discover_node(&node.use_).await?;

        // Structural check: every required input must be wired or in `with`.
        let requireds: Vec<String> = bin
            .manifest
            .inputs
            .get("required")
            .and_then(|r| r.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        for req in &requireds {
            let provided = node.with.get(req).is_some() || node.inputs.contains_key(req);
            if !provided {
                bail!(
                    "node '{}' (af-{}) is missing required input '{}'",
                    node.id,
                    node.use_,
                    req
                );
            }
        }

        // Compile the manifest's JSON Schema so we know it's valid.
        let schema = &bin.manifest.inputs;
        if schema.is_object() {
            jsonschema::validator_for(schema).map_err(|e| {
                anyhow::anyhow!("invalid JSON Schema in manifest for af-{}: {e}", node.use_)
            })?;
        }

        cache.insert(node.use_.clone(), bin);
    }
    Ok(cache)
}

async fn discover_node(use_name: &str) -> Result<NodeBin> {
    let binary = format!("af-{use_name}");
    let mut cmd = Command::new(&binary);
    cmd.arg("--describe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = cmd.spawn().with_context(|| {
        format!(
            "cannot find node binary '{binary}' — ensure it is on PATH or set AXISFLOW_NODE_PATH"
        )
    })?;
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        bail!(
            "{binary} --describe failed (exit {}): {}",
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let manifest: Manifest = serde_json::from_slice(&out.stdout)
        .with_context(|| format!("invalid manifest JSON from {binary} --describe"))?;
    Ok(NodeBin { binary, manifest })
}

// ── Main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_span_events(FmtSpan::CLOSE)
        .try_init()
        .ok();
    run().await
}

async fn run() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: axisflow <flow.yaml>")?;
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
    let flow: FlowSpec = serde_yaml::from_str(&raw).context("parse flow spec")?;

    let node_bins = preflight(&flow).await?;

    let sem = Arc::new(Semaphore::new(flow.concurrency.max(1)));
    let outputs: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut remaining: Vec<NodeSpec> = flow.nodes.clone();

    let flow_span = info_span!("flow", name = %flow.name, version = flow.version);
    let _guard = flow_span.enter();

    if !flow.description.is_empty() {
        info!(description = %flow.description, "flow description");
    }
    info!(
        nodes = remaining.len(),
        concurrency = flow.concurrency,
        "flow starting"
    );

    while !remaining.is_empty() {
        let ready_idx: Vec<usize> = {
            let guard = outputs.lock().await;
            remaining
                .iter()
                .enumerate()
                .filter(|(_, n)| deps_satisfied(n, &guard))
                .map(|(i, _)| i)
                .collect()
        };
        if ready_idx.is_empty() {
            bail!("cycle or unresolved dependency detected among remaining nodes");
        }

        let mut ready = Vec::new();
        for i in ready_idx.into_iter().rev() {
            ready.push(remaining.remove(i));
        }

        let mut handles = Vec::new();
        for n in &ready {
            let guard = outputs.lock().await;
            let input = build_input(n, &guard);
            drop(guard);
            let sem = sem.clone();
            let node = n.clone();
            let bin = node_bins
                .get(&node.use_)
                .cloned()
                .with_context(|| format!("unknown node type '{}'", node.use_))?;
            handles.push(tokio::spawn(async move {
                run_node(&node, &bin, input, sem).await
            }));
        }

        for h in handles {
            let (id, val) = h.await.context("join node task")??;
            outputs.lock().await.insert(id, val);
        }
    }

    info!("flow completed");
    Ok(())
}

// ── DAG helpers ────────────────────────────────────────────────────────────

fn deps_satisfied(node: &NodeSpec, outputs: &HashMap<String, Value>) -> bool {
    node.inputs
        .values()
        .all(|r| outputs.contains_key(upstream_of(r)))
}

fn upstream_of(refstr: &str) -> &str {
    refstr.split('.').next().unwrap_or(refstr)
}

fn build_input(node: &NodeSpec, outputs: &HashMap<String, Value>) -> Value {
    let mut base = if node.with.is_object() {
        node.with.clone()
    } else {
        Value::Object(Default::default())
    };
    if let Some(obj) = base.as_object_mut() {
        for (k, refstr) in &node.inputs {
            obj.insert(k.clone(), resolve_ref(refstr, outputs));
        }
    }
    base
}

fn resolve_ref(refstr: &str, outputs: &HashMap<String, Value>) -> Value {
    let (up, field) = match refstr.split_once('.') {
        Some((u, f)) => (u, f),
        None => (refstr, "*"),
    };
    let out = outputs.get(up).cloned().unwrap_or(Value::Null);
    if field == "*" {
        out
    } else {
        out.get(field).cloned().unwrap_or(Value::Null)
    }
}

// ── Node execution ─────────────────────────────────────────────────────────

async fn run_node(
    node: &NodeSpec,
    bin: &NodeBin,
    input: Value,
    sem: Arc<Semaphore>,
) -> Result<(String, Value)> {
    let span = info_span!("node", id = %node.id, node_type = %node.use_);
    let started = Instant::now();
    let node = node.clone();
    let bin = bin.clone();
    let result = async { execute_node(&node, &bin, input, sem).await }
        .instrument(span)
        .await;
    match &result {
        Ok(_) => info!(duration_ms = started.elapsed().as_millis(), "node ok"),
        Err(e) => {
            error!(
                error = %e,
                duration_ms = started.elapsed().as_millis(),
                "node failed"
            );
        }
    }
    result
}

async fn execute_node(
    node: &NodeSpec,
    bin: &NodeBin,
    input: Value,
    sem: Arc<Semaphore>,
) -> Result<(String, Value)> {
    let on_error = node.on_error.as_deref().unwrap_or("stop");
    let max_retries = on_error
        .strip_prefix("retry(")
        .map(|n| n.trim_end_matches(')').parse::<u32>().unwrap_or(0))
        .unwrap_or(0);

    if let Some(cond) = &node.when {
        if cond.trim() == "false" {
            info!(when = "false", "node skipped");
            return Ok((node.id.clone(), Value::Null));
        }
    }

    validate_input(&input, &bin.manifest.inputs, &node.id)?;

    let mut attempt = 0u32;
    loop {
        let _permit = sem.acquire().await.context("semaphore acquire")?;
        let mut cmd = Command::new(&bin.binary);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawn {}", bin.binary))?;
        {
            let mut stdin = child.stdin.take().context("child stdin")?;
            let bytes = serde_json::to_vec(&input)?;
            stdin.write_all(&bytes).await?;
            stdin.shutdown().await?;
        }
        let out = child.wait_with_output().await?;

        if out.status.success() {
            let val: Value = serde_json::from_slice(&out.stdout)
                .with_context(|| format!("parse output of node {}", node.id))?;
            return Ok((node.id.clone(), val));
        }

        let code = out.status.code().unwrap_or(1);
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

        if code == af_contract::exit_code::RETRYABLE && attempt < max_retries {
            attempt += 1;
            warn!(
                attempt,
                max_retries,
                exit_code = code,
                stderr,
                "node retrying"
            );
            continue;
        }
        if on_error == "continue" {
            warn!(
                exit_code = code,
                stderr,
                on_error = "continue",
                "node failed, continuing"
            );
            return Ok((node.id.clone(), Value::Null));
        }
        bail!(
            "node {} ({}) failed code {}: {}",
            node.id,
            bin.binary,
            code,
            stderr,
        );
    }
}

// ── JSON Schema validation ─────────────────────────────────────────────────

fn validate_input(input: &Value, schema: &Value, node_id: &str) -> Result<()> {
    if !schema.is_object() {
        return Ok(());
    }
    let validator = jsonschema::validator_for(schema)
        .map_err(|e| anyhow::anyhow!("invalid manifest schema for node '{node_id}': {e}"))?;
    if !validator.is_valid(input) {
        for error in validator.iter_errors(input) {
            warn!(validation_error = %error, "schema violation");
        }
        bail!("schema validation failed for node '{node_id}'");
    }
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_upstream_of() {
        assert_eq!(upstream_of("a.echo"), "a");
        assert_eq!(upstream_of("src"), "src");
        assert_eq!(upstream_of("db.rows.0.name"), "db");
    }

    #[test]
    fn test_resolve_ref_full_object() {
        let mut outputs = HashMap::new();
        outputs.insert("src".into(), json!({"echo": "hello", "count": 3}));
        assert_eq!(resolve_ref("src.echo", &outputs), json!("hello"));
        assert_eq!(resolve_ref("src.count", &outputs), json!(3));
    }

    #[test]
    fn test_resolve_ref_star() {
        let mut outputs = HashMap::new();
        outputs.insert("src".into(), json!({"echo": "hello"}));
        assert_eq!(resolve_ref("src.*", &outputs), json!({"echo": "hello"}));
    }

    #[test]
    fn test_resolve_ref_missing() {
        let outputs = HashMap::new();
        assert_eq!(resolve_ref("src.missing", &outputs), Value::Null);
    }

    #[test]
    fn test_build_input_with_and_inputs() {
        let mut outputs = HashMap::new();
        outputs.insert("a".into(), json!({"echo": "world"}));
        let node = NodeSpec {
            id: "b".into(),
            use_: "echo".into(),
            with: json!({"greeting": "hi"}),
            inputs: [("message".into(), "a.echo".into())].into(),
            when: None,
            on_error: None,
        };
        let input = build_input(&node, &outputs);
        assert_eq!(input["greeting"], json!("hi"));
        assert_eq!(input["message"], json!("world"));
    }

    #[test]
    fn test_build_input_no_with() {
        let mut outputs = HashMap::new();
        outputs.insert("a".into(), json!({"num": 42}));
        let node = NodeSpec {
            id: "b".into(),
            use_: "echo".into(),
            with: Value::Null,
            inputs: [("value".into(), "a.num".into())].into(),
            when: None,
            on_error: None,
        };
        let input = build_input(&node, &outputs);
        assert_eq!(input["value"], json!(42));
    }

    #[test]
    fn test_deps_satisfied_all_ready() {
        let mut outputs = HashMap::new();
        outputs.insert("a".into(), json!({}));
        outputs.insert("b".into(), json!({}));
        let node = NodeSpec {
            id: "c".into(),
            use_: "echo".into(),
            with: json!({}),
            inputs: [("x".into(), "a.echo".into()), ("y".into(), "b.echo".into())].into(),
            when: None,
            on_error: None,
        };
        assert!(deps_satisfied(&node, &outputs));
    }

    #[test]
    fn test_deps_satisfied_missing() {
        let mut outputs = HashMap::new();
        outputs.insert("a".into(), json!({}));
        let node = NodeSpec {
            id: "c".into(),
            use_: "echo".into(),
            with: json!({}),
            inputs: [("x".into(), "a.echo".into()), ("y".into(), "b.echo".into())].into(),
            when: None,
            on_error: None,
        };
        assert!(!deps_satisfied(&node, &outputs));
    }

    #[test]
    fn test_deps_satisfied_no_inputs() {
        let outputs = HashMap::new();
        let node = NodeSpec {
            id: "a".into(),
            use_: "echo".into(),
            with: json!({"message": "hi"}),
            inputs: Default::default(),
            when: None,
            on_error: None,
        };
        assert!(deps_satisfied(&node, &outputs));
    }

    #[test]
    fn test_validate_input_valid() {
        let schema = json!({
            "type": "object",
            "properties": { "message": { "type": "string" } },
            "required": ["message"]
        });
        let input = json!({"message": "hello"});
        assert!(validate_input(&input, &schema, "test").is_ok());
    }

    #[test]
    fn test_validate_input_wrong_type() {
        let schema = json!({
            "type": "object",
            "properties": { "message": { "type": "string" } },
            "required": ["message"]
        });
        let input = json!({"message": 42});
        assert!(validate_input(&input, &schema, "test").is_err());
    }

    #[test]
    fn test_validate_input_missing_required() {
        let schema = json!({
            "type": "object",
            "properties": { "message": { "type": "string" } },
            "required": ["message"]
        });
        let input = json!({});
        assert!(validate_input(&input, &schema, "test").is_err());
    }

    #[test]
    fn test_validate_input_no_schema() {
        let input = json!({"anything": "goes"});
        assert!(validate_input(&input, &Value::Null, "test").is_ok());
    }
}
