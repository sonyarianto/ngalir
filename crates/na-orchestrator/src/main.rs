//! Ngalir Orchestrator (v1).
//!
//! Reads a Flow Spec (YAML/JSON), discovers node binaries, validates inputs
//! against node manifests (JSON Schema), builds a DAG from `inputs`/`when`
//! wiring, then executes nodes as `na-<use>` subprocesses with bounded
//! concurrency. Inter-node data is piped as JSON via stdin/stdout.

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use na_contract::Manifest;

mod error;
mod flow;
mod init_node;
pub(crate) use flow::*;
mod cli;
pub(crate) use cli::*;
mod history;
pub(crate) use history::*;
mod state;
pub(crate) use state::*;
mod registry;
pub(crate) use registry::*;
mod vault;
pub(crate) use vault::*;
mod api;
mod oauth;
mod ws;
pub(crate) use api::*;
mod executor;
pub(crate) use executor::*;
use prometheus::{Encoder, TextEncoder};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tracing::info;
use tracing_subscriber::fmt::format::FmtSpan;

// ── Helpers ──────────────────────────────────────────────────────────────────

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
    encoder
        .encode(&prometheus::gather(), &mut buffer)
        .unwrap_or_default();
    String::from_utf8(buffer).unwrap_or_default()
}

// ── Subcommands ────────────────────────────────────────────────────────────

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

async fn cmd_serve(port: u16, ui_dir: &str) -> Result<()> {
    let (tx, _) = broadcast::channel(256);
    let (step_tx, _) = broadcast::channel(256);
    let flows_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ngalir")
        .join("flows");
    std::fs::create_dir_all(&flows_dir).ok();
    let oauth_store: crate::oauth::OAuthStore = Arc::new(std::sync::RwLock::new(HashMap::new()));
    let public_url =
        std::env::var("NGALIR_PUBLIC_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());

    let history_path = history_db_path();

    let state = crate::api::AppState {
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
        .route("/api/nodes", axum::routing::get(crate::api::api_nodes))
        .route("/api/skills", axum::routing::get(crate::api::api_skills))
        .route("/api/health", axum::routing::get(|| async { "OK" }))
        .route("/api/run", axum::routing::post(crate::api::api_run))
        .route(
            "/api/generate",
            axum::routing::post(crate::api::api_generate),
        )
        .route(
            "/api/validate",
            axum::routing::post(crate::api::api_validate),
        )
        .route(
            "/api/flows",
            axum::routing::get(crate::api::api_flows_list).post(crate::api::api_flows_save),
        )
        .route(
            "/api/flows/{name}",
            axum::routing::get(crate::api::api_flows_get)
                .put(crate::api::api_flows_update)
                .delete(crate::api::api_flows_delete),
        )
        .route(
            "/api/snapshots",
            axum::routing::get(crate::api::api_snapshots),
        )
        .route(
            "/api/snapshots/diff",
            axum::routing::get(crate::api::api_snapshots_diff),
        )
        .route(
            "/api/credentials",
            axum::routing::get(crate::api::api_credentials_list)
                .post(crate::api::api_credentials_create),
        )
        .route(
            "/api/credentials/{id}",
            axum::routing::get(crate::api::api_credentials_get)
                .put(crate::api::api_credentials_update)
                .delete(crate::api::api_credentials_delete),
        )
        .route(
            "/api/credentials/{id}/test",
            axum::routing::post(crate::api::api_credentials_test),
        )
        .route(
            "/api/oauth/{spec_id}/authorize",
            axum::routing::get(crate::oauth::api_oauth_authorize),
        )
        .route(
            "/api/oauth/callback",
            axum::routing::get(crate::oauth::api_oauth_callback),
        )
        .route(
            "/api/history",
            axum::routing::get(crate::api::api_history_list),
        )
        .route(
            "/api/history/{flow_id}",
            axum::routing::get(crate::api::api_history_get),
        )
        .route("/ws", axum::routing::get(crate::ws::ws_handler))
        .with_state(state);

    let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
    info!(port, ui_dir, "web UI server starting");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind :{port}"))?;
    axum::serve(listener, app).await?;
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
