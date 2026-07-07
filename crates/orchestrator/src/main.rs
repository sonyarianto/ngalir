//! AxisFlow Orchestrator (v1 skeleton).
//!
//! Reads a Flow Spec (YAML/JSON), builds a DAG from `inputs`/`when` wiring,
//! validates it, then executes nodes as `af-<use>` subprocesses with bounded
//! concurrency (semaphore). Inter-node data is piped as JSON via stdin/stdout.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};

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

#[tokio::main]
async fn main() -> Result<()> {
    run().await
}

async fn run() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: orchestrator <flow.yaml>")?;
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
    let flow: FlowSpec = serde_yaml::from_str(&raw).context("parse flow spec")?;

    let sem = Arc::new(Semaphore::new(flow.concurrency.max(1)));
    let outputs: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut remaining: Vec<NodeSpec> = flow.nodes.clone();

    if !flow.description.is_empty() {
        println!("[orchestrator] description: {}", flow.description);
    }
    println!(
        "[orchestrator] running '{}' v{} ({} nodes, concurrency={})",
        flow.name, flow.version, remaining.len(), flow.concurrency
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

        // Pull ready nodes out (reverse order keeps indices valid).
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
            handles.push(tokio::spawn(async move { run_node(&node, input, sem).await }));
        }

        for h in handles {
            let (id, val) = h.await.context("join node task")??;
            outputs.lock().await.insert(id, val);
        }
    }

    println!("[orchestrator] FLOW OK: '{}'", flow.name);
    Ok(())
}

/// A node is ready when every upstream it references has produced output.
fn deps_satisfied(node: &NodeSpec, outputs: &HashMap<String, Value>) -> bool {
    node.inputs
        .values()
        .all(|r| outputs.contains_key(upstream_of(r)))
}

fn upstream_of(refstr: &str) -> &str {
    refstr.split('.').next().unwrap_or(refstr)
}

/// Merge static `with` params and wired `inputs` into the node's input JSON.
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

async fn run_node(node: &NodeSpec, input: Value, sem: Arc<Semaphore>) -> Result<(String, Value)> {
    let on_error = node.on_error.as_deref().unwrap_or("stop");
    let max_retries = on_error
        .strip_prefix("retry(")
        .map(|n| n.trim_end_matches(')').parse::<u32>().unwrap_or(0))
        .unwrap_or(0);
    let binary = format!("af-{}", node.use_);

    // `when` gate: skip node if condition is explicitly false.
    if let Some(cond) = &node.when {
        if cond.trim() == "false" {
            eprintln!("[skip] node {} (when=false)", node.id);
            return Ok((node.id.clone(), Value::Null));
        }
    }

    let mut attempt = 0u32;
    loop {
        let _permit = sem.acquire().await.context("semaphore acquire")?;
        let mut cmd = Command::new(&binary);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().with_context(|| format!("spawn {binary}"))?;
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

        if code == axis_contract::exit_code::RETRYABLE && attempt < max_retries {
            attempt += 1;
            eprintln!(
                "[retry] node {} attempt {}/{} failed ({}); retrying",
                node.id, attempt, max_retries, code
            );
            continue;
        }
        if on_error == "continue" {
            eprintln!("[warn] node {} failed ({}): {}; continuing", node.id, code, stderr);
            return Ok((node.id.clone(), Value::Null));
        }
        bail!("node {} ({}) failed code {}: {}", node.id, binary, code, stderr);
    }
}
