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
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};

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

    if !flow.description.is_empty() {
        println!("[axisflow] description: {}", flow.description);
    }
    println!(
        "[axisflow] running '{}' v{} ({} nodes, concurrency={})",
        flow.name,
        flow.version,
        remaining.len(),
        flow.concurrency
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

    println!("[axisflow] FLOW OK: '{}'", flow.name);
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
    let on_error = node.on_error.as_deref().unwrap_or("stop");
    let max_retries = on_error
        .strip_prefix("retry(")
        .map(|n| n.trim_end_matches(')').parse::<u32>().unwrap_or(0))
        .unwrap_or(0);

    // `when` gate: skip node if condition is explicitly false.
    if let Some(cond) = &node.when {
        if cond.trim() == "false" {
            eprintln!("[axisflow:skip] node {} (when=false)", node.id);
            return Ok((node.id.clone(), Value::Null));
        }
    }

    // Runtime schema validation (fail fast, before spawning the process).
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
        let stderr = String::from_utf8_lossy(&out.stderr);

        if code == af_contract::exit_code::RETRYABLE && attempt < max_retries {
            attempt += 1;
            eprintln!(
                "[axisflow:retry] node {} attempt {}/{} failed ({}); retrying",
                node.id, attempt, max_retries, code
            );
            continue;
        }
        if on_error == "continue" {
            eprintln!(
                "[axisflow:warn] node {} failed ({}): {}; continuing",
                node.id, code, stderr,
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
            eprintln!("  [axisflow:validation] {node_id}: {error}");
        }
        bail!("schema validation failed for node '{node_id}'");
    }
    Ok(())
}
