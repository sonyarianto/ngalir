//! Ngalir Orchestrator (v1).
//!
//! Reads a Flow Spec (YAML/JSON), discovers node binaries, validates inputs
//! against node manifests (JSON Schema), builds a DAG from `inputs`/`when`
//! wiring, then executes nodes as `na-<use>` subprocesses with bounded
//! concurrency. Inter-node data is piped as JSON via stdin/stdout.

use anyhow::{bail, Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use na_contract::{is_leap, now_iso8601, Manifest, OAuthConfig};

mod init_node;
use prometheus::{register_int_counter_vec, Encoder, IntCounterVec, TextEncoder};
use rhai::{Engine, Scope};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;
use tokio::sync::Semaphore;
use tower_http::services::ServeDir;
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
#[command(name = "ngalir", version, about = "Flow automation engine")]
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
    /// Output the full node skills registry as JSON (for AI context)
    Skills,
    /// Generate a flow from a natural-language prompt
    Generate {
        /// Natural-language description of what the flow should do
        prompt: String,
        /// Edit an existing flow file (pass --edit path/to/flow.yaml)
        #[arg(long)]
        edit: Option<String>,
        /// LLM model to use (default: gpt-4o)
        #[arg(long)]
        model: Option<String>,
        /// Output file path (default: stdout)
        #[arg(long)]
        output: Option<String>,
    },
    /// Start the web UI server
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Directory containing built UI files
        #[arg(long, default_value = "./ui/dist")]
        ui_dir: String,
    },
    /// Analyze a flow and suggest optimizations
    Optimize {
        /// Path to Flow Spec YAML file
        flow: String,
        /// LLM model to use (default: gpt-4o)
        #[arg(long)]
        model: Option<String>,
        /// Output file path (default: stdout)
        #[arg(long)]
        output: Option<String>,
    },
    /// Generate a new node crate scaffold from interactive prompts
    InitNode,
    /// Generate shell completions
    Completion {
        /// Shell type (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },
    /// Search the node registry for available nodes
    Search {
        /// Keyword to search for (matches name, description, use_cases)
        keyword: String,
    },
    /// Install a node binary from the registry
    Install {
        /// Node name to install (e.g. "slack" installs na-slack)
        name: String,
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
        Commands::Skills => cmd_skills().await,
        Commands::Generate {
            prompt,
            edit,
            model,
            output,
        } => cmd_generate(prompt, edit, model, output).await,
        Commands::Serve { port, ui_dir } => cmd_serve(port, &ui_dir).await,
        Commands::Optimize {
            flow,
            model,
            output,
        } => cmd_optimize(&flow, model, output).await,
        Commands::InitNode => Ok(init_node::cmd_init_node()?),
        Commands::Completion { shell } => {
            generate(shell, &mut Cli::command(), "ngalir", &mut std::io::stdout());
            Ok(())
        }
        Commands::Search { keyword } => cmd_search(&keyword).await,
        Commands::Install { name } => cmd_install(&name).await,
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

/// Resolve subflow references by inlining their nodes with prefixed IDs.
fn expand_subflows(nodes: &[NodeSpec], base_dir: &std::path::Path) -> Result<Vec<NodeSpec>> {
    let mut expanded = Vec::new();
    for node in nodes {
        if !node.use_.starts_with('@') {
            expanded.push(node.clone());
            continue;
        }
        let subflow_path = base_dir.join(&node.use_[1..]);
        let subflow: FlowSpec = parse_flow(&subflow_path.to_string_lossy())?;
        let prefix = format!("{}.", node.id);

        let sub_node_ids: Vec<String> = subflow.nodes.iter().map(|n| n.id.clone()).collect();

        let mut sub_nodes: Vec<NodeSpec> = subflow
            .nodes
            .into_iter()
            .map(|mut n| {
                n.id = format!("{}{}", prefix, n.id);
                let inputs = std::mem::take(&mut n.inputs);
                n.inputs = inputs
                    .into_iter()
                    .map(|(k, v)| {
                        let prefixed = if let Some(dot) = v.find('.') {
                            let (up, field) = v.split_at(dot);
                            if sub_node_ids.iter().any(|sid| sid == up) {
                                format!("{}{}{}", prefix, up, field)
                            } else {
                                v
                            }
                        } else if sub_node_ids.iter().any(|sid| sid == &v) {
                            format!("{}{}", prefix, v)
                        } else {
                            v
                        };
                        (k, prefixed)
                    })
                    .collect();
                if let Some(w) = &n.when {
                    let mut new_w = w.clone();
                    for sn_id in &sub_node_ids {
                        let old = format!("{{{{ {}", sn_id);
                        let new = format!("{{{{ {}{}", prefix, sn_id);
                        new_w = new_w.replace(&old, &new);
                    }
                    n.when = Some(new_w);
                }
                n
            })
            .collect();

        // Map parent inputs to subflow entry nodes by local ID
        for sub_node in &mut sub_nodes {
            let local_id = sub_node.id.strip_prefix(&prefix).unwrap_or(&sub_node.id);
            if let Some(parent_val) = node.inputs.get(local_id) {
                if let Some(obj) = sub_node.with.as_object_mut() {
                    obj.insert(
                        local_id.to_string(),
                        Value::String(format!("{{{{{}}}}}", parent_val)),
                    );
                }
            }
        }

        let sub_dir = subflow_path.parent().unwrap_or(base_dir);
        let sub_nodes = expand_subflows(&sub_nodes, sub_dir)?;

        let exit_ids: Vec<String> = sub_nodes
            .iter()
            .filter(|n| n.exit)
            .map(|n| n.id.clone())
            .collect();

        if let Some(last_exit) = exit_ids.last() {
            expanded.push(NodeSpec {
                id: node.id.clone(),
                use_: "echo".to_string(),
                with: json!({"message": ""}),
                inputs: [("message".into(), format!("{}.echo", last_exit))].into(),
                when: None,
                on_error: None,
                exit: false,
                position: None,
            });
        }

        expanded.extend(sub_nodes);
    }
    Ok(expanded)
}

async fn cmd_run(path: &str, state_dir: Option<String>, input_json: Option<String>) -> Result<()> {
    let flow_path = std::path::Path::new(path);
    let base_dir = flow_path.parent().unwrap_or(std::path::Path::new("."));
    let mut flow: FlowSpec = parse_flow(path)?;
    flow.nodes = expand_subflows(&flow.nodes, base_dir)?;
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

    let history = HistoryDb::new(history_db_path()).ok().map(Arc::new);
    let flow_id = uuid::Uuid::new_v4().to_string();
    let outputs = execute_flow(
        &flow,
        &node_bins,
        &mut store,
        initial_outputs,
        None,
        None,
        history,
        &flow_id,
    )
    .await?;

    let result = serde_json::to_string(&outputs)?;
    println!("{result}");
    Ok(())
}

async fn cmd_validate(path: &str) -> Result<()> {
    let flow_path = std::path::Path::new(path);
    let base_dir = flow_path.parent().unwrap_or(std::path::Path::new("."));
    let mut flow: FlowSpec = parse_flow(path)?;
    flow.nodes = expand_subflows(&flow.nodes, base_dir)?;
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

async fn cmd_skills() -> Result<()> {
    let binaries = scan_binaries();
    let mut registry = Vec::new();
    for name in &binaries {
        let bin = match describe_binary(name).await {
            Ok(b) => b,
            Err(_) => continue,
        };
        registry.push(bin.manifest);
    }
    println!("{}", serde_json::to_string_pretty(&registry)?);
    Ok(())
}

/// Collect all node manifests into a JSON array string.
async fn skills_registry_json() -> Result<String> {
    let binaries = scan_binaries();
    let mut registry = Vec::new();
    for name in &binaries {
        if let Ok(bin) = describe_binary(name).await {
            registry.push(bin.manifest);
        }
    }
    serde_json::to_string_pretty(&registry).map_err(Into::into)
}

/// Spawn an na-* node, feed it JSON on stdin, read JSON from stdout.
async fn call_node(binary: &str, input: &Value) -> Result<Value> {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().with_context(|| format!("spawn {binary}"))?;
    {
        let mut stdin = child.stdin.take().context("child stdin")?;
        stdin.write_all(&serde_json::to_vec(input)?).await?;
        stdin.shutdown().await?;
    }
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        bail!(
            "{binary} failed (exit {}): {}",
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    serde_json::from_slice(&out.stdout).with_context(|| format!("parse {binary} output"))
}

async fn cmd_generate(
    prompt: String,
    edit: Option<String>,
    model: Option<String>,
    output: Option<String>,
) -> Result<()> {
    let registry = skills_registry_json().await?;

    let system_prompt = format!(
        "You are Ngalir, a workflow automation engine that generates YAML flow specs. \
        Your output is used directly — no explanations, no markdown, just raw YAML.\n\n\
        Available nodes:\n{registry}\n\n\
        Rules:\n\
        - version: 1\n\
        - Each node has id and use (the na-* name)\n\
        - Wire nodes via inputs: node_id.output_field\n\
        - with: for static config\n\
        - when: for conditions ({{{{ expr }}}})\n\
        - Use vault:// prefix for secrets\n\
        - Use {{{{ node_id.field }}}} for template interpolation\n\
        - Use @path for subflow references\n\
        - Output ONLY valid YAML between ```yaml and ``` markers"
    );

    let user_message = if let Some(ref flow_path) = edit {
        let existing =
            std::fs::read_to_string(flow_path).with_context(|| format!("read {flow_path}"))?;
        format!(
            "Edit this existing flow:\n\n```yaml\n{existing}\n```\n\n\
            Apply this change: {prompt}\n\n\
            Output the complete modified YAML flow between ```yaml and ``` markers."
        )
    } else {
        format!(
            "Generate a YAML flow for: {prompt}\n\n\
            Output between ```yaml and ``` markers."
        )
    };

    let llm_input = json!({
        "model": model.unwrap_or_else(|| "gpt-4o".to_string()),
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_message}
        ],
        "temperature": 0.3,
        "max_tokens": 4096,
    });

    let bin = discover_node("llm").await?;
    let result = call_node(&bin.binary, &llm_input).await?;
    let raw = result["content"].as_str().unwrap_or("").to_string();

    // Extract YAML from markdown code block
    let yaml = if let Some(start) = raw.find("```yaml") {
        let body = &raw[start + 7..];
        if let Some(end) = body.find("```") {
            body[..end].trim()
        } else {
            body.trim()
        }
    } else {
        raw.trim()
    };

    if let Some(out_path) = output {
        std::fs::write(&out_path, yaml)?;
        println!("Flow written to {out_path}");
    } else {
        print!("{yaml}");
    }

    Ok(())
}

/// Estimate the relative cost of a node type (0-100 scale).
fn estimate_node_cost(use_name: &str, _bin: Option<&Manifest>) -> u32 {
    match use_name {
        "llm" => 80,
        "db-postgres" | "db-mysql" | "db-sqlite" => 30,
        "http" => 20,
        "webhook" => 10,
        "vault" => 5,
        "email" => 5,
        "csv" | "excel" | "google-sheets" => 15,
        _ => 10,
    }
}

async fn cmd_optimize(
    flow_path: &str,
    model: Option<String>,
    output: Option<String>,
) -> Result<()> {
    let flow_raw =
        std::fs::read_to_string(flow_path).with_context(|| format!("read {flow_path}"))?;
    let flow: FlowSpec = serde_yaml::from_str(&flow_raw)
        .or_else(|_| serde_json::from_str(&flow_raw))
        .with_context(|| format!("parse {flow_path} as YAML/JSON"))?;

    // Pre-compute cost estimate and retry suggestions
    let mut cost_total = 0u32;
    let mut suggestions: Vec<String> = Vec::new();

    for node in &flow.nodes {
        let bin = if let Ok(b) = discover_node(&node.use_).await {
            Some(b.manifest)
        } else {
            None
        };

        let cost = estimate_node_cost(&node.use_, bin.as_ref());
        cost_total += cost;

        // Check if node is idempotent but lacks on_error retry
        if let Some(ref m) = bin {
            if m.idempotent && node.on_error.is_none() {
                suggestions.push(format!(
                    "node '{}' is idempotent (na-{}) — consider adding on_error: retry or a retry strategy",
                    node.id, node.use_
                ));
            }
        }

        // Check if node uses vault refs but no retry
        if node.on_error.is_none() {
            let with_str = serde_json::to_string(&node.with).unwrap_or_default();
            if with_str.contains("vault://") {
                suggestions.push(format!(
                    "node '{}' uses vault secrets — add on_error: retry for resilience against transient vault failures",
                    node.id
                ));
            }
        }
    }

    // AI optimization suggestions
    let registry = skills_registry_json().await?;
    let system_prompt = format!(
        "You are Ngalir, a workflow optimization engine. \
        Analyze the given flow and suggest improvements including:\n\
        - Parallelization opportunities (can any nodes run concurrently?)\n\
        - Error handling improvements (on_error strategies)\n\
        - Performance bottlenecks\n\
        - Alternative node choices\n\
        - Cost reduction suggestions\n\n\
        Available nodes:\n{registry}\n\n\
        Reply with a concise analysis, no YAML output needed."
    );

    let user_message = format!(
        "Analyze this flow and suggest optimizations:\n\n\
        ```yaml\n{flow_raw}\n```\n\n\
        Estimated cost score: {cost_total}/node (relative).\n\
        Initial suggestions:\n{}\n\n\
        Provide specific, actionable recommendations.",
        suggestions
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let llm_input = json!({
        "model": model.unwrap_or_else(|| "gpt-4o".to_string()),
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_message}
        ],
        "temperature": 0.3,
        "max_tokens": 2048,
    });

    let bin = discover_node("llm").await?;
    let result = call_node(&bin.binary, &llm_input).await?;
    let analysis = result["content"]
        .as_str()
        .unwrap_or("No analysis returned")
        .to_string();

    let output_text = format!(
        "Flow: {name} v{version}\n\
         Nodes: {count}\n\
         Estimated cost score: {cost}\n\n\
         -- Suggestions --\n{suggestions}\n\n\
         -- AI Analysis --\n{analysis}\n",
        name = flow.name,
        version = flow.version,
        count = flow.nodes.len(),
        cost = cost_total,
        suggestions = suggestions.join("\n"),
    );

    if let Some(out_path) = output {
        std::fs::write(&out_path, &output_text)?;
        println!("Optimization report written to {out_path}");
    } else {
        print!("{output_text}");
    }

    Ok(())
}

// ── WebSocket events ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct FlowEvent {
    r#type: String,
    flow_id: String,
    node_id: Option<String>,
    output: Option<Value>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StepCommand {
    action: String,
    flow_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct Snapshot {
    id: usize,
    timestamp: String,
    flow_name: String,
    flow_id: String,
    outputs: HashMap<String, Value>,
}

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<FlowEvent>,
    step_tx: broadcast::Sender<StepCommand>,
    snapshots: Arc<Mutex<Vec<Snapshot>>>,
    flows_dir: PathBuf,
    oauth_store: OAuthStore,
    public_url: String,
    history_path: PathBuf,
}

#[derive(Clone)]
struct StepConfig {
    flow_id: String,
    step_tx: broadcast::Sender<StepCommand>,
}

type EventFn = dyn Fn(&str, Option<&str>, Option<&Value>, Option<&str>) + Send + Sync;

// ── OAuth State ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct PendingOAuth {
    spec_id: String,
    spec_label: String,
    oauth_config: OAuthConfig,
    #[allow(dead_code)]
    created_at: std::time::Instant,
}

type OAuthStore = Arc<std::sync::RwLock<HashMap<String, PendingOAuth>>>;

fn find_oauth_spec(spec_id: &str) -> Option<(String, String, OAuthConfig)> {
    let binaries = scan_binaries();
    for name in &binaries {
        if let Ok(bin) = describe_binary_sync(name) {
            for spec in bin.manifest.credential_specs() {
                if spec.id == spec_id {
                    if let Some(oauth) = spec.oauth {
                        return Some((spec.label, bin.binary, oauth));
                    }
                }
            }
        }
    }
    None
}

fn describe_binary_sync(path: &str) -> Result<NodeBin> {
    let mut cmd = std::process::Command::new(path);
    cmd.arg("--describe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let out = cmd.output()?;
    if !out.status.success() {
        bail!("{path} --describe failed");
    }
    let manifest: Manifest = serde_json::from_slice(&out.stdout)?;
    Ok(NodeBin {
        binary: path.to_string(),
        manifest,
    })
}

async fn cmd_serve(port: u16, ui_dir: &str) -> Result<()> {
    let (tx, _) = broadcast::channel(256);
    let (step_tx, _) = broadcast::channel(256);
    let flows_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ngalir")
        .join("flows");
    std::fs::create_dir_all(&flows_dir).ok();
    let oauth_store: OAuthStore = Arc::new(std::sync::RwLock::new(HashMap::new()));
    let public_url =
        std::env::var("NGALIR_PUBLIC_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());

    let history_path = history_db_path();

    let state = AppState {
        tx,
        step_tx,
        snapshots: Arc::new(Mutex::new(Vec::new())),
        flows_dir,
        oauth_store,
        public_url,
        history_path,
    };

    let assets = ServeDir::new(ui_dir).append_index_html_on_directories(true);

    let app = axum::Router::new()
        .fallback_service(assets)
        .route("/api/nodes", axum::routing::get(api_nodes))
        .route("/api/skills", axum::routing::get(api_skills))
        .route("/api/health", axum::routing::get(|| async { "OK" }))
        .route("/api/run", axum::routing::post(api_run))
        .route("/api/generate", axum::routing::post(api_generate))
        .route("/api/validate", axum::routing::post(api_validate))
        .route(
            "/api/flows",
            axum::routing::get(api_flows_list).post(api_flows_save),
        )
        .route(
            "/api/flows/{name}",
            axum::routing::get(api_flows_get)
                .put(api_flows_update)
                .delete(api_flows_delete),
        )
        .route("/api/snapshots", axum::routing::get(api_snapshots))
        .route(
            "/api/snapshots/diff",
            axum::routing::get(api_snapshots_diff),
        )
        .route(
            "/api/credentials",
            axum::routing::get(api_credentials_list).post(api_credentials_create),
        )
        .route(
            "/api/credentials/{id}",
            axum::routing::get(api_credentials_get)
                .put(api_credentials_update)
                .delete(api_credentials_delete),
        )
        .route(
            "/api/credentials/{id}/test",
            axum::routing::post(api_credentials_test),
        )
        .route(
            "/api/oauth/{spec_id}/authorize",
            axum::routing::get(api_oauth_authorize),
        )
        .route(
            "/api/oauth/callback",
            axum::routing::get(api_oauth_callback),
        )
        .route("/api/history", axum::routing::get(api_history_list))
        .route(
            "/api/history/{flow_id}",
            axum::routing::get(api_history_get),
        )
        .route("/ws", axum::routing::get(ws_handler))
        .with_state(state);

    let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
    info!(port, ui_dir, "web UI server starting");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind :{port}"))?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Deserialize)]
struct RunRequest {
    flow: FlowSpec,
    #[serde(default)]
    flow_id: String,
    #[serde(default)]
    step: bool,
}

#[derive(Serialize)]
struct RunResponse {
    flow_id: String,
}

async fn api_run(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<RunRequest>,
) -> Result<axum::Json<RunResponse>, axum::http::StatusCode> {
    let flow_id = if req.flow_id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        req.flow_id
    };

    let tx = state.tx.clone();
    let step_tx = state.step_tx.clone();
    let flow = req.flow;
    let fid = flow_id.clone();
    let step = req.step;
    let snapshots = state.snapshots.clone();
    let flow_name = flow.name.clone();

    tokio::spawn(async move {
        if let Err(e) = run_flow_with_events(
            flow,
            fid.clone(),
            tx.clone(),
            step_tx,
            step,
            snapshots,
            flow_name,
        )
        .await
        {
            error!(flow_id = fid, error = %e, "flow execution failed");
            let _ = tx.send(FlowEvent {
                r#type: "flow_failed".into(),
                flow_id: fid,
                node_id: None,
                output: None,
                error: Some(e.to_string()),
            });
        }
    });

    Ok(axum::Json(RunResponse { flow_id }))
}

async fn run_flow_with_events(
    flow: FlowSpec,
    flow_id: String,
    tx: broadcast::Sender<FlowEvent>,
    step_tx: broadcast::Sender<StepCommand>,
    step: bool,
    snapshots: Arc<Mutex<Vec<Snapshot>>>,
    flow_name: String,
) -> Result<HashMap<String, Value>> {
    let node_bins = preflight(&flow).await?;
    let mut store = StateStore::disabled();
    let fid = flow_id.clone();

    let send =
        move |typ: &str, node_id: Option<&str>, output: Option<&Value>, error: Option<&str>| {
            let _ = tx.send(FlowEvent {
                r#type: typ.into(),
                flow_id: fid.clone(),
                node_id: node_id.map(|s| s.to_string()),
                output: output.cloned(),
                error: error.map(|s| s.to_string()),
            });
        };

    send("flow_started", None, None, None);

    for node in &flow.nodes {
        send("node_pending", Some(&node.id), None, None);
    }

    let step_cfg = step.then(|| StepConfig {
        flow_id: flow_id.clone(),
        step_tx: step_tx.clone(),
    });

    let history = HistoryDb::new(history_db_path()).ok().map(Arc::new);
    let result = execute_flow(
        &flow,
        &node_bins,
        &mut store,
        HashMap::new(),
        Some(&send),
        step_cfg.as_ref(),
        history.clone(),
        &flow_id,
    )
    .await;

    match result {
        Ok(outputs) => {
            let out_val = Value::Object(
                outputs
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
            send("flow_completed", None, Some(&out_val), None);
            let snapshot = Snapshot {
                id: 0,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs().to_string())
                    .unwrap_or_default(),
                flow_name: flow_name.clone(),
                flow_id: flow_id.clone(),
                outputs: outputs.clone(),
            };
            if let Ok(mut list) = snapshots.lock() {
                let id = list.len();
                list.push(Snapshot { id, ..snapshot });
            }
            Ok(outputs)
        }
        Err(e) => {
            send("flow_failed", None, None, Some(&e.to_string()));
            if let Some(ref h) = history {
                let _ = h.record_flow_end(&flow_id, "failed", Some(&e.to_string()));
            }
            Err(e)
        }
    }
}

#[derive(Serialize)]
struct SnapshotsResponse {
    snapshots: Vec<Snapshot>,
}

async fn api_snapshots(State(state): State<AppState>) -> axum::Json<SnapshotsResponse> {
    let list = state.snapshots.lock().unwrap_or_else(|e| e.into_inner());
    axum::Json(SnapshotsResponse {
        snapshots: list.clone(),
    })
}

#[derive(Deserialize)]
struct DiffQuery {
    from: usize,
    to: usize,
}

#[derive(Serialize)]
struct DiffEntry {
    node_id: String,
    from: Option<Value>,
    to: Option<Value>,
    changed: bool,
}

#[derive(Serialize)]
struct DiffResponse {
    from: Snapshot,
    to: Snapshot,
    diffs: Vec<DiffEntry>,
}

async fn api_snapshots_diff(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<DiffQuery>,
) -> Result<axum::Json<DiffResponse>, axum::http::StatusCode> {
    let list = state.snapshots.lock().unwrap_or_else(|e| e.into_inner());
    let from = list
        .get(query.from)
        .cloned()
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    let to = list
        .get(query.to)
        .cloned()
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let mut all_keys: BTreeSet<&str> = BTreeSet::new();
    for k in from.outputs.keys().chain(to.outputs.keys()) {
        all_keys.insert(k.as_str());
    }

    let diffs: Vec<DiffEntry> = all_keys
        .into_iter()
        .map(|k| {
            let fv = from.outputs.get(k);
            let tv = to.outputs.get(k);
            DiffEntry {
                node_id: k.to_string(),
                from: fv.cloned(),
                to: tv.cloned(),
                changed: fv != tv,
            }
        })
        .collect();

    Ok(axum::Json(DiffResponse { from, to, diffs }))
}

// ── Flow CRUD ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct FlowMeta {
    name: String,
    modified: String,
}

#[derive(Serialize)]
struct FlowsListResponse {
    flows: Vec<FlowMeta>,
}

async fn api_flows_list(State(state): State<AppState>) -> axum::Json<FlowsListResponse> {
    let mut flows = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&state.flows_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    let modified = path
                        .metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| {
                            t.duration_since(std::time::UNIX_EPOCH)
                                .ok()
                                .map(|d| d.as_secs().to_string())
                        })
                        .unwrap_or_default();
                    flows.push(FlowMeta {
                        name: name.to_string(),
                        modified,
                    });
                }
            }
        }
    }
    flows.sort_by(|a, b| b.modified.cmp(&a.modified));
    axum::Json(FlowsListResponse { flows })
}

#[derive(Serialize)]
struct FlowSaveResponse {
    name: String,
}

async fn api_flows_save(
    State(state): State<AppState>,
    axum::Json(flow): axum::Json<FlowSpec>,
) -> Result<axum::Json<FlowSaveResponse>, axum::http::StatusCode> {
    let name = if flow.name.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        flow.name.clone()
    };
    let path = state.flows_dir.join(format!("{name}.json"));
    let content = serde_json::to_string_pretty(&flow)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    std::fs::write(&path, content).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(FlowSaveResponse { name }))
}

async fn api_flows_get(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<axum::Json<FlowSpec>, axum::http::StatusCode> {
    let path = state.flows_dir.join(format!("{name}.json"));
    let content = std::fs::read_to_string(&path).map_err(|_| axum::http::StatusCode::NOT_FOUND)?;
    let flow: FlowSpec = serde_json::from_str(&content)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(flow))
}

#[derive(Deserialize)]
struct FlowUpdate {
    flow: FlowSpec,
}

async fn api_flows_update(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    axum::Json(req): axum::Json<FlowUpdate>,
) -> Result<axum::Json<FlowSaveResponse>, axum::http::StatusCode> {
    let path = state.flows_dir.join(format!("{name}.json"));
    if !path.exists() {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }
    let content = serde_json::to_string_pretty(&req.flow)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    std::fs::write(&path, content).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(FlowSaveResponse { name }))
}

async fn api_flows_delete(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let path = state.flows_dir.join(format!("{name}.json"));
    if !path.exists() {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }
    std::fs::remove_file(&path).map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(serde_json::json!({"deleted": name})))
}

// ── API Generate & Validate ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct GenerateRequest {
    prompt: String,
    #[serde(default)]
    model: Option<String>,
}

async fn api_generate(
    State(_state): State<AppState>,
    axum::Json(req): axum::Json<GenerateRequest>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    let registry = skills_registry_json()
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let system_prompt = format!(
        "You are Ngalir, a workflow automation engine. \
        Generate a YAML flow spec for the given task.\n\n\
        Available nodes:\n{registry}\n\n\
        Output ONLY valid YAML between ```yaml and ``` markers."
    );

    let llm_input = json!({
        "model": req.model.unwrap_or_else(|| "gpt-4o".to_string()),
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": req.prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 4096,
    });

    let bin = discover_node("llm")
        .await
        .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;
    let result = call_node(&bin.binary, &llm_input)
        .await
        .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;
    let raw = result["content"].as_str().unwrap_or("").to_string();

    let yaml = if let Some(start) = raw.find("```yaml") {
        let body = &raw[start + 7..];
        if let Some(end) = body.find("```") {
            body[..end].trim()
        } else {
            body.trim()
        }
    } else {
        raw.trim()
    };

    Ok(axum::Json(serde_json::json!({
        "yaml": yaml,
        "raw": raw,
    })))
}

#[derive(Deserialize)]
struct ValidateRequest {
    flow: FlowSpec,
}

#[derive(Serialize)]
struct ValidateIssue {
    node_id: String,
    severity: String,
    message: String,
}

#[derive(Serialize)]
struct ValidateResponse {
    valid: bool,
    issues: Vec<ValidateIssue>,
}

async fn api_validate(
    State(_state): State<AppState>,
    axum::Json(req): axum::Json<ValidateRequest>,
) -> axum::Json<ValidateResponse> {
    let mut issues = Vec::new();

    match check_cycles(&req.flow.nodes) {
        Ok(()) => {}
        Err(e) => issues.push(ValidateIssue {
            node_id: "flow".into(),
            severity: "error".into(),
            message: e.to_string(),
        }),
    }

    // Check for duplicate node IDs
    let mut seen = std::collections::HashSet::new();
    for node in &req.flow.nodes {
        if !seen.insert(node.id.clone()) {
            issues.push(ValidateIssue {
                node_id: node.id.clone(),
                severity: "error".into(),
                message: format!("duplicate node id '{}'", node.id),
            });
        }
    }

    // Validate inputs reference existing nodes
    for node in &req.flow.nodes {
        for (key, refstr) in &node.inputs {
            let upstream = upstream_of(refstr);
            if !req.flow.nodes.iter().any(|n| n.id == upstream) {
                issues.push(ValidateIssue {
                    node_id: node.id.clone(),
                    severity: "error".into(),
                    message: format!("input '{}' references unknown node '{}'", key, upstream),
                });
            }
        }
    }

    // Check that required nodes are available
    for node in &req.flow.nodes {
        if !node.use_.starts_with('@') {
            match discover_node(&node.use_).await {
                Ok(_) => {}
                Err(_) => issues.push(ValidateIssue {
                    node_id: node.id.clone(),
                    severity: "warning".into(),
                    message: format!("node type 'na-{}' not found on PATH", node.use_),
                }),
            }
        }
    }

    axum::Json(ValidateResponse {
        valid: issues.is_empty(),
        issues,
    })
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    let rx = state.tx.subscribe();
    let step_tx = state.step_tx.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, rx, step_tx))
}

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<FlowEvent>,
    step_tx: broadcast::Sender<StepCommand>,
) {
    let mut closed = false;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        if let Ok(text) = serde_json::to_string(&event) {
                            if socket.send(Message::Text(text.into())).await.is_err() {
                                closed = true;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(lagged = n, "websocket client lagged behind");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        closed = true;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<StepCommand>(&text) {
                            let _ = step_tx.send(cmd);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        closed = true;
                    }
                    _ => {
                        closed = true;
                    }
                }
            }
        }

        if closed {
            break;
        }
    }
}

async fn api_nodes() -> axum::Json<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    for name in scan_binaries() {
        if let Ok(bin) = describe_binary(&name).await {
            out.push(serde_json::json!({
                "name": bin.manifest.name,
                "version": bin.manifest.version,
                "description": bin.manifest.description,
                "streaming": bin.manifest.streaming,
                "idempotent": bin.manifest.idempotent,
                "use_cases": bin.manifest.use_cases,
            }));
        }
    }
    axum::Json(out)
}

async fn api_skills() -> axum::Json<Vec<serde_json::Value>> {
    let mut out = Vec::new();
    for name in scan_binaries() {
        if let Ok(bin) = describe_binary(&name).await {
            out.push(serde_json::to_value(&bin.manifest).unwrap_or_default());
        }
    }
    axum::Json(out)
}

// ── Flow Spec types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlowSpec {
    version: u32,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    nodes: Vec<NodeSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<NoteSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NoteSpec {
    id: String,
    text: String,
    position: Position,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    height: f64,
    #[serde(default)]
    color: String,
}

fn default_concurrency() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// If true, this node is a subflow exit point (its output becomes the subflow's output).
    #[serde(default)]
    exit: bool,
    /// UI canvas position (preserved across save/load for the visual editor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position: Option<Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Position {
    x: f64,
    y: f64,
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

#[allow(clippy::too_many_arguments)]
async fn execute_flow(
    flow: &FlowSpec,
    node_bins: &HashMap<String, NodeBin>,
    store: &mut StateStore,
    initial_outputs: HashMap<String, Value>,
    on_event: Option<&EventFn>,
    step_cfg: Option<&StepConfig>,
    history: Option<Arc<HistoryDb>>,
    flow_id: &str,
) -> Result<HashMap<String, Value>> {
    let sem = Arc::new(Semaphore::new(flow.concurrency.max(1)));
    let mut merged = store.data.clone();
    merged.extend(initial_outputs);
    let outputs: Arc<tokio::sync::Mutex<HashMap<String, Value>>> =
        Arc::new(tokio::sync::Mutex::new(merged));
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
    if let Some(ref h) = history {
        let _ = h.record_flow_start(flow_id, &flow.name, remaining.len());
    }
    FLOW_EXECUTIONS.with_label_values(&["started"]).inc();
    let flow_started = Instant::now();
    let error_count = 0u64;
    let node_types: HashMap<String, String> = flow
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.use_.clone()))
        .collect();

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
            if let Some(ref h) = history {
                let _ =
                    h.record_flow_end(flow_id, "failed", Some("cycle or unresolved dependency"));
            }
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
                    if let Some(f) = on_event {
                        f("node_skipped", Some(&n.id), Some(&val), None);
                    }
                    if let Some(ref h) = history {
                        let _ = h.record_node_skipped(flow_id, &n.id, &n.use_);
                    }
                    if resumed {
                        store.insert(n.id.clone(), val);
                        store.save()?;
                    }
                    continue;
                }
            }

            interpolate_json(&mut input, &guard);
            drop(guard);

            if let Some(f) = on_event {
                f("node_input_ready", Some(&n.id), Some(&input), None);
            }

            if let Some(f) = on_event {
                f("node_started", Some(&n.id), None, None);
            }
            if let Some(ref h) = history {
                let _ = h.record_node_start(flow_id, &n.id, &n.use_, Some(&input));
            }

            let sem = sem.clone();
            let node = n.clone();
            let node_id_for_event = n.id.clone();
            let bin = match node_bins.get(&node.use_).cloned() {
                Some(b) => b,
                None => {
                    let err = format!("unknown node type '{}'", node.use_);
                    if let Some(f) = on_event {
                        f("node_failed", Some(&node.id), None, Some(&err));
                    }
                    bail!("{err}");
                }
            };
            let output_dir = output_dir_path.clone();
            handles.push(tokio::spawn(async move {
                let result = run_node(&node, &bin, input, sem, &output_dir).await;
                (node_id_for_event, result)
            }));
        }

        for h in handles {
            let (node_id, inner) = h.await.context("join node task")?;
            let node_type = node_types.get(&node_id).map(|s| s.as_str()).unwrap_or("?");
            match inner {
                Ok((id, val)) => {
                    if let Some(f) = on_event {
                        f("node_completed", Some(&node_id), Some(&val), None);
                    }
                    if let Some(ref h) = history {
                        let _ = h.record_node_end(
                            flow_id,
                            &node_id,
                            node_type,
                            "completed",
                            Some(&val),
                            None,
                        );
                    }
                    outputs.lock().await.insert(id.clone(), val.clone());
                    if resumed {
                        store.insert(id, val);
                        store.save()?;
                    }
                }
                Err(e) => {
                    if let Some(f) = on_event {
                        f("node_failed", Some(&node_id), None, Some(&e.to_string()));
                    }
                    if let Some(ref h) = history {
                        let _ = h.record_node_end(
                            flow_id,
                            &node_id,
                            node_type,
                            "failed",
                            None,
                            Some(&e.to_string()),
                        );
                    }
                    return Err(e);
                }
            }
        }

        if let Some(cfg) = step_cfg {
            if let Some(f) = on_event {
                f("step_ready", None, None, None);
            }
            let mut rx = cfg.step_tx.subscribe();
            loop {
                match rx.recv().await {
                    Ok(cmd) if cmd.flow_id == cfg.flow_id => match cmd.action.as_str() {
                        "stop" => {
                            if let Some(ref h) = history {
                                let _ = h.record_flow_end(flow_id, "stopped", None);
                            }
                            let final_outputs = outputs.lock().await.clone();
                            return Ok(final_outputs);
                        }
                        "continue" => break,
                        _ => {}
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                    _ => continue,
                }
            }
        }
    }

    info!("flow completed");
    FLOW_EXECUTIONS.with_label_values(&["completed"]).inc();
    if let Some(ref h) = history {
        let _ = h.record_flow_end(flow_id, "completed", None);
    }
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
    let vault_bin = find_in_node_path("na-vault").unwrap_or_else(|| "na-vault".into());
    let mut cmd = Command::new(&vault_bin);
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

// ── Credential CRUD helpers ───────────────────────────────────────────────

async fn call_vault(mode: &str, id: Option<&str>, stdin_data: Option<Value>) -> Result<Value> {
    let vault_bin = find_in_node_path("na-vault").unwrap_or_else(|| "na-vault".into());
    let mut cmd = Command::new(&vault_bin);
    cmd.arg(mode)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(arg) = id {
        cmd.arg(arg);
    }

    let mut child = cmd.spawn().context("spawn na-vault (is it on PATH?)")?;

    if let Some(data) = stdin_data {
        let mut stdin = child.stdin.take().context("na-vault stdin")?;
        let bytes = serde_json::to_vec(&data)?;
        stdin.write_all(&bytes).await?;
        stdin.shutdown().await?;
    } else {
        drop(child.stdin.take());
    }

    let out = child.wait_with_output().await?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let msg = if stderr.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            stderr
        };
        bail!("na-vault {} failed: {}", mode, msg);
    }

    let result: Value = serde_json::from_slice(&out.stdout)
        .with_context(|| format!("parse na-vault {} output", mode))?;
    Ok(result)
}

async fn call_vault_list() -> Result<Vec<Value>> {
    let result = call_vault("--list", None, None).await?;
    serde_json::from_value(result).context("na-vault list: expected array")
}

async fn call_vault_get(id: &str) -> Result<Value> {
    call_vault("--get", Some(id), None).await
}

async fn call_vault_create(data: Value) -> Result<Value> {
    call_vault("--create", None, Some(data)).await
}

async fn call_vault_update(id: &str, data: Value) -> Result<Value> {
    call_vault("--update", Some(id), Some(data)).await
}

async fn call_vault_delete(id: &str) -> Result<Value> {
    call_vault("--delete", Some(id), None).await
}

// ── Credential API endpoints ──────────────────────────────────────────────

#[derive(Serialize)]
struct CredentialListResponse {
    credentials: Vec<Value>,
}

async fn api_credentials_list() -> Result<axum::Json<CredentialListResponse>, axum::http::StatusCode>
{
    match call_vault_list().await {
        Ok(list) => Ok(axum::Json(CredentialListResponse { credentials: list })),
        Err(e) => {
            error!("credential list failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn api_credentials_get(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<Value>, axum::http::StatusCode> {
    match call_vault_get(&id).await {
        Ok(cred) => Ok(axum::Json(cred)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                Err(axum::http::StatusCode::NOT_FOUND)
            } else {
                error!("credential get failed: {e}");
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

#[derive(Deserialize)]
struct CreateCredentialRequest {
    credential_spec_id: String,
    label: String,
    #[serde(default)]
    auth_type: String,
    #[serde(default)]
    data: serde_json::Map<String, Value>,
}

async fn api_credentials_create(
    axum::Json(req): axum::Json<CreateCredentialRequest>,
) -> Result<(axum::http::StatusCode, axum::Json<Value>), axum::http::StatusCode> {
    let data = serde_json::json!({
        "credential_spec_id": req.credential_spec_id,
        "label": req.label,
        "auth_type": req.auth_type,
        "data": req.data,
    });

    match call_vault_create(data).await {
        Ok(cred) => Ok((axum::http::StatusCode::CREATED, axum::Json(cred))),
        Err(e) => {
            error!("credential create failed: {e}");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct UpdateCredentialRequest {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    auth_type: Option<String>,
    #[serde(default)]
    data: Option<serde_json::Map<String, Value>>,
}

async fn api_credentials_update(
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(req): axum::Json<UpdateCredentialRequest>,
) -> Result<axum::Json<Value>, axum::http::StatusCode> {
    let mut data = serde_json::Map::new();
    if let Some(label) = req.label {
        data.insert("label".into(), Value::String(label));
    }
    if let Some(auth_type) = req.auth_type {
        data.insert("auth_type".into(), Value::String(auth_type));
    }
    if let Some(d) = req.data {
        data.insert("data".into(), Value::Object(d));
    }

    match call_vault_update(&id, Value::Object(data)).await {
        Ok(cred) => Ok(axum::Json(cred)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                Err(axum::http::StatusCode::NOT_FOUND)
            } else {
                error!("credential update failed: {e}");
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

async fn api_credentials_delete(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<Value>, axum::http::StatusCode> {
    match call_vault_delete(&id).await {
        Ok(result) => Ok(axum::Json(result)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                Err(axum::http::StatusCode::NOT_FOUND)
            } else {
                error!("credential delete failed: {e}");
                Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

async fn api_credentials_test(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<Value>, axum::http::StatusCode> {
    let cred = match call_vault_get(&id).await {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                return Err(axum::http::StatusCode::NOT_FOUND);
            }
            error!("credential get for test failed: {e}");
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let credential_spec_id = cred["credential_spec_id"].as_str().unwrap_or("");

    // Find a node binary that matches this credential_spec_id
    let binaries = scan_binaries();
    let mut test_bin: Option<String> = None;
    for name in &binaries {
        if let Ok(bin) = describe_binary(name).await {
            let specs = bin.manifest.credential_specs();
            if specs.iter().any(|s| s.id == credential_spec_id) {
                test_bin = Some(bin.binary.clone());
                break;
            }
        }
    }

    let binary = match test_bin {
        Some(b) => b,
        None => {
            return Ok(axum::Json(serde_json::json!({
                "ok": false,
                "message": format!("no node found for credential spec '{credential_spec_id}'")
            })));
        }
    };

    // Extract credential data fields and pass to node's --test-connection
    let data = cred
        .get("data")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let test_input = Value::Object(data);

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg("--test-connection")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(axum::Json(serde_json::json!({
                "ok": false,
                "message": format!("cannot spawn {binary}: {e}")
            })));
        }
    };

    {
        let mut stdin = match child.stdin.take() {
            Some(s) => s,
            None => {
                return Ok(axum::Json(serde_json::json!({
                    "ok": false,
                    "message": "failed to open stdin for test connection"
                })));
            }
        };
        let _ = stdin
            .write_all(&serde_json::to_vec(&test_input).unwrap_or_default())
            .await;
        let _ = stdin.shutdown().await;
    }

    let out = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => {
            return Ok(axum::Json(serde_json::json!({
                "ok": false,
                "message": format!("test connection failed: {e}")
            })));
        }
    };

    match serde_json::from_slice::<Value>(&out.stdout) {
        Ok(result) => Ok(axum::Json(result)),
        Err(_) => Ok(axum::Json(serde_json::json!({
            "ok": out.status.success(),
            "message": String::from_utf8_lossy(&out.stdout).trim().to_string(),
        }))),
    }
}

// ── OAuth Endpoints ─────────────────────────────────────────────────────────

/// Redirect the user to the OAuth provider's authorization page.
async fn api_oauth_authorize(
    State(state): State<AppState>,
    axum::extract::Path(spec_id): axum::extract::Path<String>,
) -> Result<axum::response::Redirect, axum::http::StatusCode> {
    let (label, _binary, oauth_config) =
        find_oauth_spec(&spec_id).ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let client_id = std::env::var(&oauth_config.client_id_env).map_err(|_| {
        error!(
            env = oauth_config.client_id_env,
            "OAuth client_id env var not set"
        );
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let state_token = uuid::Uuid::new_v4().to_string();
    let pending = PendingOAuth {
        spec_id: spec_id.clone(),
        spec_label: label,
        oauth_config: oauth_config.clone(),
        created_at: std::time::Instant::now(),
    };
    if let Ok(mut store) = state.oauth_store.write() {
        store.insert(state_token.clone(), pending);
    }

    let redirect_uri = format!("{}/api/oauth/callback", state.public_url);
    let scopes = oauth_config.scopes.join(" ");

    let mut params = vec![
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("state", state_token),
        ("response_type", "code".to_string()),
    ];
    if !scopes.is_empty() {
        params.push(("scope", scopes));
    }

    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let auth_url = format!("{}?{}", oauth_config.authorize_url, query);
    info!(spec_id, %auth_url, "oauth redirect");
    Ok(axum::response::Redirect::to(&auth_url))
}

/// Handle the OAuth callback: exchange code for tokens and store credential.
async fn api_oauth_callback(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Result<axum::response::Redirect, axum::http::StatusCode> {
    let state_token = params.get("state").ok_or_else(|| {
        error!("oauth callback missing state param");
        axum::http::StatusCode::BAD_REQUEST
    })?;
    let code = params.get("code").ok_or_else(|| {
        error!("oauth callback missing code param");
        axum::http::StatusCode::BAD_REQUEST
    })?;

    let pending = {
        let mut store = state.oauth_store.write().map_err(|e| {
            error!(error = %e, "oauth store lock failed");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;
        store.remove(state_token).ok_or_else(|| {
            error!(state = %state_token, "oauth state not found or expired");
            axum::http::StatusCode::BAD_REQUEST
        })?
    };

    let client_id = std::env::var(&pending.oauth_config.client_id_env).map_err(|_| {
        error!(
            env = pending.oauth_config.client_id_env,
            "oauth client_id env var not set"
        );
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let secret_env = pending
        .oauth_config
        .client_secret_env
        .clone()
        .unwrap_or_else(|| {
            // Derive from client_id_env: NGALIR_SLACK_CLIENT_ID -> NGALIR_SLACK_CLIENT_SECRET
            let suffix = pending
                .oauth_config
                .client_id_env
                .strip_suffix("_ID")
                .unwrap_or(&pending.oauth_config.client_id_env);
            format!("{}_SECRET", suffix)
        });

    let client_secret = std::env::var(&secret_env).map_err(|_| {
        error!(env = secret_env, "oauth client_secret env var not set");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let redirect_uri = format!("{}/api/oauth/callback", state.public_url);

    // Exchange code for token
    let client = reqwest::Client::new();
    let token_params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
    ];

    let token_resp = match client
        .post(&pending.oauth_config.token_url)
        .form(&token_params)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "oauth token exchange request failed");
            let err_url = format!(
                "{}?oauth_error={}",
                state.public_url,
                urlencode("Token exchange request failed")
            );
            return Ok(axum::response::Redirect::to(&err_url));
        }
    };

    let token_body: Value = match token_resp.json().await {
        Ok(j) => j,
        Err(e) => {
            error!(error = %e, "oauth token exchange parse failed");
            let err_url = format!(
                "{}?oauth_error={}",
                state.public_url,
                urlencode("Invalid token response")
            );
            return Ok(axum::response::Redirect::to(&err_url));
        }
    };

    let access_token = match token_body["access_token"].as_str() {
        Some(t) => t.to_string(),
        None => {
            error!(body = %token_body, "oauth token response missing access_token");
            let err_url = format!(
                "{}?oauth_error={}",
                state.public_url,
                urlencode("No access token in response")
            );
            return Ok(axum::response::Redirect::to(&err_url));
        }
    };

    let refresh_token = token_body["refresh_token"].as_str().map(String::from);

    // Store credential in vault
    let mut data = serde_json::Map::new();
    data.insert("access_token".into(), Value::String(access_token));
    if let Some(rt) = refresh_token {
        data.insert("refresh_token".into(), Value::String(rt));
    }
    if let Some(expires_in) = token_body["expires_in"].as_i64() {
        data.insert("expires_in".into(), Value::Number(expires_in.into()));
    }

    let create_req = serde_json::json!({
        "credential_spec_id": pending.spec_id,
        "label": format!("{} (OAuth)", pending.spec_label),
        "auth_type": "oauth2",
        "data": data,
    });

    match call_vault_create(create_req).await {
        Ok(cred) => {
            let cred_id = cred["id"].as_str().unwrap_or("unknown");
            info!(cred_id = %cred_id, spec_id = pending.spec_id, "oauth credential created");
            let ok_url = format!("{}?oauth_success={}", state.public_url, urlencode(cred_id));
            Ok(axum::response::Redirect::to(&ok_url))
        }
        Err(e) => {
            error!(error = %e, "oauth failed to save credential in vault");
            let err_url = format!(
                "{}?oauth_error={}",
                state.public_url,
                urlencode("Failed to save credential")
            );
            Ok(axum::response::Redirect::to(&err_url))
        }
    }
}

// ── History API endpoints ─────────────────────────────────────────────────

async fn api_history_list(
    State(state): State<AppState>,
) -> Result<axum::Json<Value>, axum::http::StatusCode> {
    let db = HistoryDb::new(state.history_path.clone()).map_err(|e| {
        error!(error = %e, "failed to open history db");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match db.list_runs() {
        Ok(runs) => Ok(axum::Json(serde_json::json!({"runs": runs}))),
        Err(e) => {
            error!(error = %e, "history list failed");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn api_history_get(
    State(state): State<AppState>,
    axum::extract::Path(flow_id): axum::extract::Path<String>,
) -> Result<axum::Json<Value>, axum::http::StatusCode> {
    let db = HistoryDb::new(state.history_path.clone()).map_err(|e| {
        error!(error = %e, "failed to open history db");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match db.get_run(&flow_id) {
        Ok(Some(run)) => Ok(axum::Json(run)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(e) => {
            error!(error = %e, "history get failed");
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}

// ── Execution History ─────────────────────────────────────────────────────────

fn chrono_now() -> String {
    now_iso8601()
}

fn history_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("NGALIR_HISTORY_FILE") {
        return PathBuf::from(p);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ngalir")
        .join("history.db")
}

struct HistoryDb {
    path: PathBuf,
}

impl HistoryDb {
    fn new(path: PathBuf) -> Result<Self> {
        let db = Self { path };
        db.init()?;
        Ok(db)
    }

    fn connect(&self) -> Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Ok(conn)
    }

    fn init(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS flow_runs (
                flow_id       TEXT PRIMARY KEY,
                flow_name     TEXT NOT NULL,
                status        TEXT NOT NULL,
                started_at    TEXT NOT NULL,
                finished_at   TEXT,
                duration_ms   INTEGER,
                node_count    INTEGER NOT NULL DEFAULT 0,
                error         TEXT
            );
            CREATE TABLE IF NOT EXISTS node_runs (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                flow_id       TEXT NOT NULL,
                node_id       TEXT NOT NULL,
                node_type     TEXT NOT NULL,
                status        TEXT NOT NULL,
                started_at    TEXT,
                finished_at   TEXT,
                duration_ms   INTEGER,
                input         TEXT,
                output        TEXT,
                error         TEXT,
                UNIQUE(flow_id, node_id)
            );",
        )?;
        Ok(())
    }

    fn record_flow_start(&self, flow_id: &str, flow_name: &str, node_count: usize) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono_now();
        conn.execute(
            "INSERT OR REPLACE INTO flow_runs (flow_id, flow_name, status, started_at, node_count)
             VALUES (?1, ?2, 'running', ?3, ?4)",
            rusqlite::params![flow_id, flow_name, now, node_count],
        )?;
        Ok(())
    }

    fn record_flow_end(&self, flow_id: &str, status: &str, error: Option<&str>) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono_now();
        let mut duration_ms: Option<i64> = None;
        if let Ok(start_row) = conn.query_row(
            "SELECT started_at FROM flow_runs WHERE flow_id = ?1",
            rusqlite::params![flow_id],
            |row| row.get::<_, String>(0),
        ) {
            if let (Ok(start), Ok(end)) = (parse_iso8601_ms(&start_row), parse_iso8601_ms(&now)) {
                duration_ms = Some(end - start);
            }
        }
        conn.execute(
            "UPDATE flow_runs SET status = ?1, finished_at = ?2, duration_ms = ?3, error = ?4 WHERE flow_id = ?5",
            rusqlite::params![status, now, duration_ms, error, flow_id],
        )?;
        Ok(())
    }

    fn record_node_start(
        &self,
        flow_id: &str,
        node_id: &str,
        node_type: &str,
        input: Option<&Value>,
    ) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono_now();
        let input_str = input.map(|v| v.to_string());
        conn.execute(
            "INSERT OR REPLACE INTO node_runs (flow_id, node_id, node_type, status, started_at, input)
             VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
            rusqlite::params![flow_id, node_id, node_type, now, input_str],
        )?;
        Ok(())
    }

    fn record_node_end(
        &self,
        flow_id: &str,
        node_id: &str,
        _node_type: &str,
        status: &str,
        output: Option<&Value>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.connect()?;
        let now = chrono_now();
        let output_str = output.map(|v| v.to_string());
        let mut duration_ms: Option<i64> = None;
        if let Ok(Some(start_str)) = conn.query_row(
            "SELECT started_at FROM node_runs WHERE flow_id = ?1 AND node_id = ?2",
            rusqlite::params![flow_id, node_id],
            |row| row.get::<_, Option<String>>(0),
        ) {
            if let (Ok(start), Ok(end)) = (parse_iso8601_ms(&start_str), parse_iso8601_ms(&now)) {
                duration_ms = Some(end - start);
            }
        }
        conn.execute(
            "UPDATE node_runs SET status = ?1, finished_at = ?2, duration_ms = ?3, output = ?4, error = ?5
             WHERE flow_id = ?6 AND node_id = ?7",
            rusqlite::params![status, now, duration_ms, output_str, error, flow_id, node_id],
        )?;
        Ok(())
    }

    fn record_node_skipped(&self, flow_id: &str, node_id: &str, node_type: &str) -> Result<()> {
        let now = chrono_now();
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO node_runs (flow_id, node_id, node_type, status, started_at, finished_at)
             VALUES (?1, ?2, ?3, 'skipped', ?4, ?5)",
            rusqlite::params![flow_id, node_id, node_type, now, now],
        )?;
        Ok(())
    }

    fn list_runs(&self) -> Result<Vec<Value>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT flow_id, flow_name, status, started_at, finished_at, duration_ms, node_count, error
             FROM flow_runs ORDER BY started_at DESC LIMIT 100",
        )?;
        let rows = stmt.query_map([], |row| {
            let flow_id: String = row.get(0)?;
            let flow_name: String = row.get(1)?;
            let status: String = row.get(2)?;
            let started_at: String = row.get(3)?;
            let finished_at: Option<String> = row.get(4)?;
            let duration_ms: Option<i64> = row.get(5)?;
            let node_count: i64 = row.get(6)?;
            let error: Option<String> = row.get(7)?;
            Ok(serde_json::json!({
                "flow_id": flow_id,
                "flow_name": flow_name,
                "status": status,
                "started_at": started_at,
                "finished_at": finished_at,
                "duration_ms": duration_ms,
                "node_count": node_count,
                "error": error,
            }))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    fn get_run(&self, flow_id: &str) -> Result<Option<Value>> {
        let conn = self.connect()?;
        let flow = conn
            .query_row(
                "SELECT flow_id, flow_name, status, started_at, finished_at, duration_ms, node_count, error
                 FROM flow_runs WHERE flow_id = ?1",
                rusqlite::params![flow_id],
                |row| {
                    let flow_id: String = row.get(0)?;
                    let flow_name: String = row.get(1)?;
                    let status: String = row.get(2)?;
                    let started_at: String = row.get(3)?;
                    let finished_at: Option<String> = row.get(4)?;
                    let duration_ms: Option<i64> = row.get(5)?;
                    let node_count: i64 = row.get(6)?;
                    let error: Option<String> = row.get(7)?;
                    Ok(serde_json::json!({
                        "flow_id": flow_id,
                        "flow_name": flow_name,
                        "status": status,
                        "started_at": started_at,
                        "finished_at": finished_at,
                        "duration_ms": duration_ms,
                        "node_count": node_count,
                        "error": error,
                    }))
                },
            )
            .ok();

        let Some(flow) = flow else { return Ok(None) };

        let mut stmt = conn.prepare(
            "SELECT node_id, node_type, status, started_at, finished_at, duration_ms, input, output, error
             FROM node_runs WHERE flow_id = ?1 ORDER BY id ASC",
        )?;
        let node_rows = stmt.query_map(rusqlite::params![flow_id], |row| {
            let node_id: String = row.get(0)?;
            let node_type: String = row.get(1)?;
            let status: String = row.get(2)?;
            let started_at: Option<String> = row.get(3)?;
            let finished_at: Option<String> = row.get(4)?;
            let duration_ms: Option<i64> = row.get(5)?;
            let input: Option<String> = row.get(6)?;
            let output: Option<String> = row.get(7)?;
            let error: Option<String> = row.get(8)?;
            Ok(serde_json::json!({
                "node_id": node_id,
                "node_type": node_type,
                "status": status,
                "started_at": started_at,
                "finished_at": finished_at,
                "duration_ms": duration_ms,
                "input": input.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "output": output.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "error": error,
            }))
        })?;
        let mut nodes = Vec::new();
        for row in node_rows {
            nodes.push(row?);
        }

        Ok(Some(serde_json::json!({
            "flow": flow,
            "nodes": nodes,
        })))
    }
}

fn parse_iso8601_ms(s: &str) -> Result<i64> {
    // Format: 2026-07-22T12:34:56Z
    if s.len() < 20 {
        bail!("invalid timestamp: {s}");
    }
    let y: i64 = s[0..4].parse()?;
    let m: i64 = s[5..7].parse()?;
    let d: i64 = s[8..10].parse()?;
    let h: i64 = s[11..13].parse()?;
    let min: i64 = s[14..16].parse()?;
    let sec: i64 = s[17..19].parse()?;
    let days = days_since_epoch(y, m, d);
    Ok((days * 86400 + h * 3600 + min * 60 + sec) * 1000)
}

fn days_since_epoch(y: i64, m: i64, d: i64) -> i64 {
    let mut total = 0i64;
    for year in 1970..y {
        total += if is_leap(year) { 366 } else { 365 };
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    for days in month_days.iter().take(m as usize - 1) {
        total += days;
    }
    total + d - 1
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

// ── Registry ────────────────────────────────────────────────────────────────

const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/sonyarianto/ngalir/main/docs/registry.json";

#[derive(Debug, Deserialize)]
struct RegistryEntry {
    name: String,
    version: String,
    description: String,
    #[serde(default)]
    use_cases: Vec<String>,
    #[allow(dead_code)]
    repo: String,
}

async fn fetch_registry() -> Result<Vec<RegistryEntry>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    match client.get(REGISTRY_URL).send().await {
        Ok(resp) if resp.status().is_success() => {
            let entries: Vec<RegistryEntry> = resp.json().await?;
            return Ok(entries);
        }
        _ => {}
    }
    let local = Path::new("docs/registry.json");
    if local.exists() {
        let raw = std::fs::read_to_string(local)?;
        let entries: Vec<RegistryEntry> = serde_json::from_str(&raw)?;
        return Ok(entries);
    }
    bail!("could not fetch registry from {REGISTRY_URL} and docs/registry.json not found locally");
}

async fn cmd_search(keyword: &str) -> Result<()> {
    let kw = keyword.to_lowercase();
    let entries = fetch_registry().await?;
    let matched: Vec<&RegistryEntry> = entries
        .iter()
        .filter(|e| {
            e.name.to_lowercase().contains(&kw)
                || e.description.to_lowercase().contains(&kw)
                || e.use_cases.iter().any(|u| u.to_lowercase().contains(&kw))
        })
        .collect();

    if matched.is_empty() {
        println!("No nodes found matching \"{keyword}\".");
        println!(
            "Registry has {} node(s). Try a broader keyword.",
            entries.len()
        );
        return Ok(());
    }

    println!("{} node(s) matching \"{}\":\n", matched.len(), keyword);
    for e in &matched {
        let short = e.name.strip_prefix("na-").unwrap_or(&e.name);
        let uc = if e.use_cases.is_empty() {
            String::new()
        } else {
            format!(" [{}]", e.use_cases.join(", "))
        };
        println!("  {short:12} v{:<8} — {}{uc}", e.version, e.description);
    }
    Ok(())
}

async fn cmd_install(name: &str) -> Result<()> {
    let node_name = if name.starts_with("na-") {
        name.to_string()
    } else {
        format!("na-{name}")
    };

    let entries = fetch_registry().await?;
    let _entry = entries
        .iter()
        .find(|e| e.name == node_name)
        .ok_or_else(|| anyhow::anyhow!("node '{node_name}' not found in registry"))?;

    let target = detect_target();
    println!("Installing {} for {} ...", node_name, target);

    let install_dir = determine_install_dir()?;
    std::fs::create_dir_all(&install_dir)?;
    let dest = install_dir.join(&node_name);

    let repo = "sonyarianto/ngalir";
    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");

    let client = reqwest::Client::builder()
        .user_agent("ngalir")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let release: Value = client.get(&api_url).send().await?.json().await?;
    let tag = release["tag_name"]
        .as_str()
        .context("failed to get latest release tag")?;

    let asset_name = format!("ngalir-{tag}-{target}.tar.gz");
    let dl_url = format!("https://github.com/{repo}/releases/download/{tag}/{asset_name}");

    println!("  downloading {asset_name} ...");
    let mut resp = client
        .get(&dl_url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!(
            "download failed: HTTP {} (asset not found: {asset_name})",
            resp.status()
        );
    }

    let tmp = tempfile::NamedTempFile::new()?;
    let tmp_path = tmp.path().to_owned();

    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        bytes.extend_from_slice(&chunk);
    }

    let cursor = std::io::Cursor::new(&bytes);
    let decoder = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(decoder);
    let mut found = false;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path == std::path::Path::new(&node_name) {
            let mut out = std::fs::File::create(&tmp_path)?;
            std::io::copy(&mut entry, &mut out)?;
            found = true;
            break;
        }
    }

    if !found {
        bail!("binary '{node_name}' not found in release archive");
    }

    std::fs::set_permissions(
        &tmp_path,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )?;
    std::fs::rename(&tmp_path, &dest)?;

    println!("  installed to {}", dest.display());

    if let Some(parent) = dest.parent() {
        if !on_path(parent) {
            println!(
                "  warning: {} is not on PATH. Add it or move the binary.",
                parent.display()
            );
        }
    }
    Ok(())
}

fn detect_target() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    match (arch, os) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu".into(),
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu".into(),
        ("x86_64", "macos") => "x86_64-apple-darwin".into(),
        ("aarch64", "macos") => "aarch64-apple-darwin".into(),
        _ => format!("{arch}-unknown-{os}-gnu"),
    }
}

fn determine_install_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("NGALIR_INSTALL_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let local_bin = home.join(".local").join("bin");
    if local_bin.exists() {
        return Ok(local_bin);
    }
    Ok(home.join(".local").join("bin"))
}

fn on_path(dir: &Path) -> bool {
    std::env::var("PATH")
        .ok()
        .map(|p| std::env::split_paths(&p).any(|d| d == dir))
        .unwrap_or(false)
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
            exit: false,
            position: None,
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
            exit: false,
            position: None,
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
            exit: false,
            position: None,
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
            exit: false,
            position: None,
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
            exit: false,
            position: None,
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
            exit: false,
            position: None,
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
            exit: false,
            position: None,
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
