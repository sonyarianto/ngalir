//! Ngalir Orchestrator (v1).
//!
//! Reads a Flow Spec (YAML/JSON), discovers node binaries, validates inputs
//! against node manifests (JSON Schema), builds a DAG from `inputs`/`when`
//! wiring, then executes nodes as `na-<use>` subprocesses with bounded
//! concurrency. Inter-node data is piped as JSON via stdin/stdout.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use na_contract::Manifest;
use prometheus::{register_int_counter_vec, Encoder, IntCounterVec, TextEncoder};
use rhai::{Engine, Scope};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, Semaphore};
use tracing::{error, info, info_span, warn, Instrument};
use tracing_subscriber::fmt::format::FmtSpan;

static FLOW_EXECUTIONS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "ngalir_flow_executions_total",
        "Total flow executions",
        &["status"]
    )
    .unwrap()
});

static NODE_EXECUTIONS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "ngalir_node_executions_total",
        "Total node executions",
        &["node_type", "status"]
    )
    .unwrap()
});

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn read_stream_output<R>(reader: R) -> Result<Vec<Value>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut lines = reader.lines();
    let mut stream = Vec::new();
    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(val) => stream.push(val),
            Err(e) => {
                warn!(error = %e, line = line, "skipping unparseable NDJSON line");
            }
        }
    }
    Ok(stream)
}
use tokio::process::Command;

// ── CLI ────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "ngalir",
    version,
    about = "n8n-like flow engine, built in Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a Flow Spec
    Run {
        /// Path to Flow Spec YAML file
        flow: String,
        /// Directory for checkpoint state files (enables resume on restart)
        #[arg(long)]
        state_dir: Option<String>,
        /// JSON input string to inject as `__request__` for the flow
        #[arg(long)]
        input: Option<String>,
        /// Port for Prometheus metrics HTTP server (disabled if 0)
        #[arg(long, default_value_t = 0)]
        metrics_port: u16,
    },
    /// List all available na-* node binaries on PATH / NGALIR_NODE_PATH
    Nodes,
    /// Validate a Flow Spec without executing it
    Validate {
        /// Path to Flow Spec YAML file
        flow: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            flow,
            state_dir,
            input,
            metrics_port,
        } => {
            tracing_subscriber::fmt()
                .json()
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(std::io::stderr)
                .try_init()
                .ok();
            if metrics_port > 0 {
                tokio::spawn(async move {
                    let app = axum::Router::new()
                        .route("/health", axum::routing::get(|| async { "OK" }))
                        .route("/metrics", axum::routing::get(metrics_handler));
                    let addr: SocketAddr = ([0, 0, 0, 0], metrics_port).into();
                    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
                        info!(metrics_port, "orchestrator metrics server starting");
                        axum::serve(listener, app).await.ok();
                    }
                });
            }
            cmd_run(&flow, state_dir, input).await
        }
        Commands::Nodes => cmd_nodes().await,
        Commands::Validate { flow } => cmd_validate(&flow).await,
    }
}

// ── Prometheus metrics ─────────────────────────────────────────────────────

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&prometheus::gather(), &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap_or_default()
}

// ── Subcommands ────────────────────────────────────────────────────────────

async fn cmd_run(path: &str, state_dir: Option<String>, input_json: Option<String>) -> Result<()> {
    let flow: FlowSpec = parse_flow(path)?;
    check_cycles(&flow.nodes)?;
    let node_bins = preflight(&flow).await?;
    let mut store = match state_dir {
        Some(dir) => {
            StateStore::load_or_new(PathBuf::from(dir).join(format!("{}.json", flow.name)))
        }
        None => StateStore::disabled(),
    };

    let initial_outputs = input_json
        .map(|s| serde_json::from_str::<Value>(&s))
        .transpose()?
        .map(|v| {
            let mut m = HashMap::new();
            m.insert("__request__".to_string(), v);
            m
        })
        .unwrap_or_default();

    let outputs = execute_flow(&flow, &node_bins, &mut store, initial_outputs).await?;

    let result = serde_json::to_string(&outputs)?;
    println!("{result}");
    Ok(())
}

async fn cmd_validate(path: &str) -> Result<()> {
    let flow: FlowSpec = parse_flow(path)?;
    check_cycles(&flow.nodes)?;
    let bins = preflight(&flow).await?;
    println!("Flow '{}' is valid.", flow.name);
    println!("Node types required:");
    for (use_, bin) in &bins {
        println!(
            "  na-{}  v{}  — {}",
            use_, bin.manifest.version, bin.manifest.description
        );
    }
    Ok(())
}

async fn cmd_nodes() -> Result<()> {
    let binaries = scan_binaries();
    if binaries.is_empty() {
        println!("No na-* node binaries found on PATH or NGALIR_NODE_PATH.");
        return Ok(());
    }
    println!("{} node(s) detected:\n", binaries.len());
    for name in &binaries {
        let bin = match describe_binary(name).await {
            Ok(b) => b,
            Err(e) => {
                println!("  {name}  (—describe error: {e})");
                continue;
            }
        };
        let short = bin
            .manifest
            .name
            .strip_prefix("na-")
            .unwrap_or(&bin.manifest.name);
        println!(
            "  na-{short:12} v{:<8} — {}",
            bin.manifest.version, bin.manifest.description
        );
    }
    Ok(())
}

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

fn parse_flow(path: &str) -> Result<FlowSpec> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    serde_yaml::from_str(&raw).context("parse flow spec")
}

// ── Binary discovery ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct NodeBin {
    binary: String,
    manifest: Manifest,
}

async fn preflight(flow: &FlowSpec) -> Result<HashMap<String, NodeBin>> {
    let mut cache = HashMap::new();
    for node in &flow.nodes {
        if cache.contains_key(&node.use_) {
            continue;
        }
        let bin = discover_node(&node.use_).await?;

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
                    "node '{}' (na-{}) is missing required input '{}'",
                    node.id,
                    node.use_,
                    req
                );
            }
        }

        let schema = &bin.manifest.inputs;
        if schema.is_object() {
            jsonschema::validator_for(schema).map_err(|e| {
                anyhow::anyhow!("invalid JSON Schema in manifest for na-{}: {e}", node.use_)
            })?;
        }

        cache.insert(node.use_.clone(), bin);
    }
    Ok(cache)
}

async fn discover_node(use_name: &str) -> Result<NodeBin> {
    let binary = format!("na-{use_name}");
    if let Some(full) = find_in_node_path(&binary) {
        return describe_binary(&full).await;
    }
    describe_binary(&binary).await
}

fn find_in_node_path(name: &str) -> Option<String> {
    let node_path = std::env::var("NGALIR_NODE_PATH").ok()?;
    for dir in std::env::split_paths(&node_path) {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

async fn describe_binary(path: &str) -> Result<NodeBin> {
    let mut cmd = Command::new(path);
    cmd.arg("--describe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = cmd.spawn().with_context(|| {
        format!("cannot find node binary '{path}' — ensure it is on PATH or set NGALIR_NODE_PATH")
    })?;
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        bail!(
            "{path} --describe failed (exit {}): {}",
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let manifest: Manifest = serde_json::from_slice(&out.stdout)
        .with_context(|| format!("invalid manifest JSON from {path} --describe"))?;
    Ok(NodeBin {
        binary: path.to_string(),
        manifest,
    })
}

fn scan_binaries() -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("NGALIR_NODE_PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    if let Ok(p) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("na-") && is_executable(&e) {
                seen.insert(name);
            }
        }
    }
    seen.into_iter().collect()
}

#[cfg(unix)]
fn is_executable(entry: &std::fs::DirEntry) -> bool {
    use std::os::unix::fs::PermissionsExt;
    entry
        .metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(entry: &std::fs::DirEntry) -> bool {
    entry.metadata().map(|m| m.is_file()).unwrap_or(false)
}

// ── Cycle detection ───────────────────────────────────────────────────────

fn check_cycles(nodes: &[NodeSpec]) -> Result<()> {
    // Map node id -> index for fast lookup
    let idx_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();

    // Build adjacency list using indices (inputs + when refs)
    let deps_of: Vec<Vec<usize>> = nodes
        .iter()
        .map(|n| {
            let mut deps: Vec<usize> = n
                .inputs
                .values()
                .filter_map(|r| idx_of.get(upstream_of(r)).copied())
                .collect();
            if let Some(w) = &n.when {
                for u in refs_in_str(w) {
                    if let Some(&i) = idx_of.get(u) {
                        deps.push(i);
                    }
                }
            }
            deps
        })
        .collect();

    // Find successor indices (reverse of deps)
    let succ_of: Vec<Vec<usize>> = {
        let mut succ = vec![vec![]; nodes.len()];
        for (i, deps) in deps_of.iter().enumerate() {
            for &d in deps {
                succ[d].push(i);
            }
        }
        succ
    };

    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    fn dfs(
        i: usize,
        succ: &[Vec<usize>],
        color: &mut Vec<Color>,
        path: &mut Vec<usize>,
        names: &[String],
    ) -> Result<()> {
        color[i] = Color::Gray;
        path.push(i);
        for &next in &succ[i] {
            match color[next] {
                Color::Gray => {
                    let start = path.iter().position(|&n| n == next).unwrap_or(0);
                    let cycle: Vec<&str> =
                        path[start..].iter().map(|&n| names[n].as_str()).collect();
                    bail!("cycle detected: {} -> {}", cycle.join(" -> "), names[next]);
                }
                Color::White => dfs(next, succ, color, path, names)?,
                Color::Black => {}
            }
        }
        path.pop();
        color[i] = Color::Black;
        Ok(())
    }

    let mut color = vec![Color::White; nodes.len()];
    let mut path: Vec<usize> = Vec::new();
    let names: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();

    for i in 0..nodes.len() {
        if color[i] == Color::White {
            dfs(i, &succ_of, &mut color, &mut path, &names)?;
        }
    }
    Ok(())
}

// ── Checkpoint / Resume state store ────────────────────────────────────────

struct StateStore {
    path: Option<PathBuf>,
    data: HashMap<String, Value>,
}

impl StateStore {
    fn disabled() -> Self {
        Self {
            path: None,
            data: HashMap::new(),
        }
    }

    fn load_or_new(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            data,
        }
    }

    fn contains(&self, id: &str) -> bool {
        self.data.contains_key(id)
    }

    fn insert(&mut self, id: String, value: Value) {
        self.data.insert(id, value);
    }

    fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Atomic write: write to temp file, then rename
        let tmp = path.with_extension("tmp");
        let json = serde_json::to_string(&self.data)?;
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

// ── Flow execution ─────────────────────────────────────────────────────────

async fn execute_flow(
    flow: &FlowSpec,
    node_bins: &HashMap<String, NodeBin>,
    store: &mut StateStore,
    initial_outputs: HashMap<String, Value>,
) -> Result<HashMap<String, Value>> {
    let sem = Arc::new(Semaphore::new(flow.concurrency.max(1)));
    let mut merged = store.data.clone();
    merged.extend(initial_outputs);
    let outputs: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(merged));
    let mut remaining: Vec<NodeSpec> = flow.nodes.clone();

    let output_dir = tempfile::TempDir::new()?;
    let output_dir_path = Arc::new(output_dir.path().to_path_buf());

    // Remove already-checkpointed nodes from the worklist
    let resumed = store.path.is_some();
    if resumed {
        let skip_ids: Vec<String> = remaining
            .iter()
            .filter(|n| store.contains(&n.id))
            .map(|n| n.id.clone())
            .collect();
        remaining.retain(|n| !store.contains(&n.id));
        if !skip_ids.is_empty() {
            info!(skipped = ?skip_ids, "resumed from checkpoint, skipping completed nodes");
        }
    }

    let flow_span = info_span!("flow", name = %flow.name, version = flow.version);
    let _guard = flow_span.enter();

    if !flow.description.is_empty() {
        info!(description = %flow.description, "flow description");
    }
    info!(
        nodes = remaining.len(),
        concurrency = flow.concurrency,
        resumed,
        "flow starting"
    );
    FLOW_EXECUTIONS.with_label_values(&["started"]).inc();
    let flow_started = Instant::now();
    let error_count = 0u64;

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
            let mut input = build_input(n, &guard);

            if let Some(cond) = &n.when {
                if !eval_when(cond, &guard)? {
                    info!(id = n.id, when = cond, "node skipped");
                    drop(guard);
                    let val = Value::Null;
                    outputs.lock().await.insert(n.id.clone(), val.clone());
                    if resumed {
                        store.insert(n.id.clone(), val);
                        store.save()?;
                    }
                    continue;
                }
            }

            interpolate_json(&mut input, &guard);
            drop(guard);

            let sem = sem.clone();
            let node = n.clone();
            let bin = node_bins
                .get(&node.use_)
                .cloned()
                .with_context(|| format!("unknown node type '{}'", node.use_))?;
            let output_dir = output_dir_path.clone();
            handles.push(tokio::spawn(async move {
                run_node(&node, &bin, input, sem, &output_dir).await
            }));
        }

        for h in handles {
            let (id, val) = h.await.context("join node task")??;
            outputs.lock().await.insert(id.clone(), val.clone());
            if resumed {
                store.insert(id, val);
                store.save()?;
            }
        }
    }

    info!("flow completed");
    FLOW_EXECUTIONS.with_label_values(&["completed"]).inc();
    info!(
        metric = "flow.duration",
        duration_ms = flow_started.elapsed().as_millis(),
        node_count = flow.nodes.len(),
        error_count,
        "flow metrics"
    );
    let final_outputs = outputs.lock().await.clone();
    Ok(final_outputs)
}

// ── DAG helpers ────────────────────────────────────────────────────────────

/// Resolve a `__file__`-tagged output by reading the file contents.
fn resolve_file_output(val: Value) -> Value {
    if let Some(file_path) = val.as_str() {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
                return parsed;
            }
        }
        return val;
    }
    if let Some(obj) = val.as_object() {
        let mut new_obj = serde_json::Map::new();
        for (k, v) in obj {
            if k == "__file__" {
                if let Some(path) = v.as_str() {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
                            return parsed;
                        }
                    }
                }
            }
            new_obj.insert(k.clone(), resolve_file_output(v.clone()));
        }
        return Value::Object(new_obj);
    }
    val
}

fn deps_satisfied(node: &NodeSpec, outputs: &HashMap<String, Value>) -> bool {
    node.inputs
        .values()
        .all(|r| outputs.contains_key(upstream_of(r)))
        && node
            .when
            .as_deref()
            .map(|w| refs_in_str(w).iter().all(|&u| outputs.contains_key(u)))
            .unwrap_or(true)
}

fn refs_in_str(s: &str) -> Vec<&str> {
    let mut refs = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("}}") {
            let ref_str = rest[..end].trim();
            if let Some(dot) = ref_str.find('.') {
                refs.push(&ref_str[..dot]);
            } else {
                refs.push(ref_str);
            }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    refs
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

// ── Expression evaluation & interpolation ─────────────────────────────────

fn eval_when(condition: &str, outputs: &HashMap<String, Value>) -> Result<bool> {
    let engine = Engine::new();
    let mut scope = Scope::new();

    for (id, val) in outputs {
        let dyn_val = rhai::serde::to_dynamic(val.clone())
            .map_err(|e| anyhow::anyhow!("failed to convert output '{}' for when: {}", id, e))?;
        scope.push_constant_dynamic(id.clone(), dyn_val);
    }

    let expr = condition.trim();
    let expr = expr
        .strip_prefix("{{")
        .and_then(|s| s.strip_suffix("}}"))
        .map(|s| s.trim())
        .unwrap_or(expr);

    if expr.is_empty() || expr == "true" {
        return Ok(true);
    }
    if expr == "false" {
        return Ok(false);
    }

    let result: bool = engine
        .eval_expression_with_scope(&mut scope, expr)
        .map_err(|e| anyhow::anyhow!("when expression '{}' failed: {e}", expr))?;
    Ok(result)
}

fn interpolate_json(value: &mut Value, outputs: &HashMap<String, Value>) {
    match value {
        Value::String(s) => {
            *s = interpolate_str(s, outputs);
        }
        Value::Object(obj) => {
            for v in obj.values_mut() {
                interpolate_json(v, outputs);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                interpolate_json(v, outputs);
            }
        }
        _ => {}
    }
}

fn interpolate_str(s: &str, outputs: &HashMap<String, Value>) -> String {
    let mut result = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("}}") {
            let ref_str = rest[..end].trim();
            rest = &rest[end + 2..];
            match resolve_ref(ref_str, outputs) {
                Value::String(v) => result.push_str(&v),
                Value::Null => {}
                other => result.push_str(&other.to_string()),
            }
        } else {
            result.push('{');
            result.push('{');
            result.push_str(rest);
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result
}

// ── Node execution ─────────────────────────────────────────────────────────

async fn run_node(
    node: &NodeSpec,
    bin: &NodeBin,
    input: Value,
    sem: Arc<Semaphore>,
    output_dir: &Path,
) -> Result<(String, Value)> {
    let span = info_span!("node", id = %node.id, node_type = %node.use_);
    let started = Instant::now();
    let node = node.clone();
    let bin = bin.clone();
    let result = async { execute_node(&node, &bin, input, sem, output_dir).await }
        .instrument(span)
        .await;
    match &result {
        Ok(_) => {
            NODE_EXECUTIONS
                .with_label_values(&[&node.use_, "success"])
                .inc();
            info!(duration_ms = started.elapsed().as_millis(), "node ok");
        }
        Err(e) => {
            NODE_EXECUTIONS
                .with_label_values(&[&node.use_, "failed"])
                .inc();
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
    output_dir: &Path,
) -> Result<(String, Value)> {
    let on_error = node.on_error.as_deref().unwrap_or("stop");
    let max_retries = on_error
        .strip_prefix("retry(")
        .map(|n| n.trim_end_matches(')').parse::<u32>().unwrap_or(0))
        .unwrap_or(0);

    validate_input(&input, &bin.manifest.inputs, &node.id)?;
    let mut input = input;
    resolve_vault_refs(&mut input).await?;

    // Strip secret fields from input JSON and inject as child-process env vars
    let mut secrets: HashMap<String, String> = HashMap::new();
    if let Some(obj) = input.as_object() {
        for name in &bin.manifest.secrets {
            if let Some(val) = obj.get(name) {
                let env_val = match val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                secrets.insert(name.clone(), env_val);
            }
        }
    }
    for name in secrets.keys() {
        if let Some(obj) = input.as_object_mut() {
            obj.remove(name.as_str());
        }
    }

    let mut attempt = 0u32;
    loop {
        let _permit = sem.acquire().await.context("semaphore acquire")?;
        let mut cmd = Command::new(&bin.binary);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (name, val) in &secrets {
            cmd.env(format!("NGALIR_SECRET_{}", name.to_uppercase()), val);
        }
        if bin.manifest.output_is_file() {
            cmd.env(
                "NGALIR_OUTPUT_DIR",
                output_dir.to_string_lossy().to_string(),
            );
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawn {}", bin.binary))?;
        {
            let mut stdin = child.stdin.take().context("child stdin")?;
            let bytes = serde_json::to_vec(&input)?;
            stdin.write_all(&bytes).await?;
            stdin.shutdown().await?;
        }

        if bin.manifest.streaming {
            let stdout = child.stdout.take().context("child stdout")?;
            let reader = BufReader::new(stdout);
            let stream = read_stream_output(reader).await?;
            // Read stderr
            let mut stderr = String::new();
            if let Some(stderr_pipe) = child.stderr.take() {
                tokio::io::BufReader::new(stderr_pipe)
                    .read_to_string(&mut stderr)
                    .await?;
            }

            let status = child.wait().await?;
            if status.success() {
                let val = if bin.manifest.output_is_file() {
                    resolve_file_output(json!({"stream": stream}))
                } else {
                    json!({"stream": stream})
                };
                return Ok((node.id.clone(), val));
            }
            let code = status.code().unwrap_or(1);
            if code == na_contract::exit_code::RETRYABLE && attempt < max_retries {
                attempt += 1;
                let delay_ms = 100u64 * 2u64.pow(attempt - 1);
                warn!(
                    attempt,
                    max_retries,
                    exit_code = code,
                    delay_ms,
                    stderr,
                    "node retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
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
                stderr
            );
        } else {
            let out = child.wait_with_output().await?;

            if out.status.success() {
                let val: Value = serde_json::from_slice(&out.stdout)
                    .with_context(|| format!("parse output of node {}", node.id))?;
                let val = if bin.manifest.output_is_file() {
                    resolve_file_output(val)
                } else {
                    val
                };
                return Ok((node.id.clone(), val));
            }

            let code = out.status.code().unwrap_or(1);
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

            if code == na_contract::exit_code::RETRYABLE && attempt < max_retries {
                attempt += 1;
                let delay_ms = 100u64 * 2u64.pow(attempt - 1);
                warn!(
                    attempt,
                    max_retries,
                    exit_code = code,
                    delay_ms,
                    stderr,
                    "node retrying"
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
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
}

// ── Vault resolution ──────────────────────────────────────────────────────

async fn resolve_vault_refs(input: &mut Value) -> Result<()> {
    resolve_vault_recursive(input).await
}

fn has_vault_prefix(s: &str) -> Option<&str> {
    s.strip_prefix("vault://").filter(|k| !k.is_empty())
}

async fn resolve_vault_recursive(value: &mut Value) -> Result<()> {
    match value {
        Value::Object(obj) => {
            for (_, v) in obj.iter_mut() {
                Box::pin(resolve_vault_recursive(v)).await?;
            }
        }
        Value::String(s) if has_vault_prefix(s).is_some() => {
            let resolved = call_vault_resolve(s).await?;
            *s = resolved;
        }
        _ => {}
    }
    Ok(())
}

async fn call_vault_resolve(ref_str: &str) -> Result<String> {
    let mut cmd = Command::new("na-vault");
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().context("spawn na-vault (is it on PATH?)")?;
    {
        let mut stdin = child.stdin.take().context("na-vault stdin")?;
        let req = serde_json::json!({"ref": ref_str});
        let bytes = serde_json::to_vec(&req)?;
        stdin.write_all(&bytes).await?;
        stdin.shutdown().await?;
    }
    let out = child.wait_with_output().await?;

    if !out.status.success() {
        bail!(
            "na-vault resolve failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let result: Value = serde_json::from_slice(&out.stdout).context("parse na-vault output")?;
    Ok(result["secret"]
        .as_str()
        .context("na-vault response missing 'secret' field")?
        .to_string())
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

    #[test]
    fn test_eval_when_true() {
        let outputs = HashMap::new();
        assert!(eval_when("true", &outputs).unwrap());
        assert!(eval_when("{{true}}", &outputs).unwrap());
    }

    #[test]
    fn test_eval_when_false() {
        let outputs = HashMap::new();
        assert!(!eval_when("false", &outputs).unwrap());
        assert!(!eval_when("{{false}}", &outputs).unwrap());
    }

    #[test]
    fn test_eval_when_expression() {
        let mut outputs = HashMap::new();
        outputs.insert("src".into(), json!({"count": 10}));
        assert!(eval_when("{{ src.count > 5 }}", &outputs).unwrap());
        assert!(!eval_when("{{ src.count < 5 }}", &outputs).unwrap());
        assert!(eval_when("{{ src.count == 10 }}", &outputs).unwrap());
    }

    #[test]
    fn test_eval_when_boolean_logic() {
        let mut outputs = HashMap::new();
        outputs.insert("a".into(), json!({"ok": true}));
        outputs.insert("b".into(), json!({"done": false}));
        assert!(eval_when("{{ a.ok && !b.done }}", &outputs).unwrap());
        assert!(!eval_when("{{ !a.ok }}", &outputs).unwrap());
        assert!(eval_when("{{ a.ok || b.done }}", &outputs).unwrap());
    }

    #[test]
    fn test_refs_in_str_empty() {
        assert!(refs_in_str("hello world").is_empty());
    }

    #[test]
    fn test_refs_in_str_single() {
        let refs = refs_in_str("{{ src.echo }} > 5");
        assert_eq!(refs, vec!["src"]);
    }

    #[test]
    fn test_refs_in_str_multiple() {
        let mut refs = refs_in_str("{{ a.x }} && {{ b.y }}");
        refs.sort();
        assert_eq!(refs, vec!["a", "b"]);
    }

    #[test]
    fn test_deps_satisfied_with_when_refs() {
        let mut outputs = HashMap::new();
        outputs.insert("src".into(), json!({}));
        let node = NodeSpec {
            id: "check".into(),
            use_: "echo".into(),
            with: json!({}),
            inputs: Default::default(),
            when: Some("{{ src.count > 5 }}".into()),
            on_error: None,
        };
        assert!(deps_satisfied(&node, &outputs));
    }

    #[test]
    fn test_deps_satisfied_with_when_refs_missing() {
        let outputs = HashMap::new();
        let node = NodeSpec {
            id: "check".into(),
            use_: "echo".into(),
            with: json!({}),
            inputs: Default::default(),
            when: Some("{{ src.count > 5 }}".into()),
            on_error: None,
        };
        assert!(!deps_satisfied(&node, &outputs));
    }

    #[test]
    fn test_interpolate_str_no_templates() {
        let outputs = HashMap::new();
        assert_eq!(interpolate_str("hello world", &outputs), "hello world");
    }

    #[test]
    fn test_interpolate_str_simple() {
        let mut outputs = HashMap::new();
        outputs.insert("a".into(), json!({"name": "Alice"}));
        assert_eq!(
            interpolate_str("Hello {{ a.name }}!", &outputs),
            "Hello Alice!"
        );
    }

    #[test]
    fn test_interpolate_str_numeric() {
        let mut outputs = HashMap::new();
        outputs.insert("x".into(), json!({"val": 42}));
        assert_eq!(interpolate_str("count={{ x.val }}", &outputs), "count=42");
    }

    #[test]
    fn test_interpolate_json_nested() {
        let mut outputs = HashMap::new();
        outputs.insert("env".into(), json!({"host": "example.com", "port": 8080}));
        let mut input = json!({
            "url": "https://{{ env.host }}:{{ env.port }}/api",
            "meta": { "name": "static" }
        });
        interpolate_json(&mut input, &outputs);
        assert_eq!(input["url"], "https://example.com:8080/api");
        assert_eq!(input["meta"]["name"], "static");
    }

    #[tokio::test]
    async fn test_read_stream_output_valid_lines() {
        let data = r#"{"a":1}
{"b":2}
{"c":3}
"#;
        let stream = read_stream_output(tokio::io::BufReader::new(std::io::Cursor::new(data)))
            .await
            .unwrap();
        assert_eq!(stream.len(), 3);
        assert_eq!(stream[0], json!({"a": 1}));
        assert_eq!(stream[1], json!({"b": 2}));
        assert_eq!(stream[2], json!({"c": 3}));
    }

    #[tokio::test]
    async fn test_read_stream_output_skips_empty_lines() {
        let data = r#"{"a":1}

{"b":2}

"#;
        let stream = read_stream_output(tokio::io::BufReader::new(std::io::Cursor::new(data)))
            .await
            .unwrap();
        assert_eq!(stream.len(), 2);
        assert_eq!(stream[0], json!({"a": 1}));
        assert_eq!(stream[1], json!({"b": 2}));
    }

    #[tokio::test]
    async fn test_read_stream_output_skips_invalid_json() {
        let data = "not json\n{\"valid\": true}\ntrash\n";
        let stream = read_stream_output(tokio::io::BufReader::new(std::io::Cursor::new(data)))
            .await
            .unwrap();
        assert_eq!(stream.len(), 1);
        assert_eq!(stream[0], json!({"valid": true}));
    }

    #[tokio::test]
    async fn test_read_stream_output_empty() {
        let stream = read_stream_output(tokio::io::BufReader::new(std::io::Cursor::new("")))
            .await
            .unwrap();
        assert!(stream.is_empty());
    }
}
