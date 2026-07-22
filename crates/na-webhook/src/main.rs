use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use na_contract::{print_manifest, Manifest};
use prometheus::{register_int_counter_vec, Encoder, IntCounterVec, TextEncoder};
use serde_json::Value;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::LazyLock;
use tokio::process::Command;
use tracing::{error, info};

static FLOW_EXECUTIONS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "na_webhook_flow_executions_total",
        "Total flow executions triggered via webhook",
        &["status"]
    )
    .unwrap()
});

fn manifest() -> Manifest {
    Manifest {
        name: "na-webhook".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "HTTP server that executes a flow on each POST request.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "port": { "type": "integer", "default": 8080 },
                "path": { "type": "string", "default": "/" },
                "flow": { "type": "string" }
            },
            "required": ["flow"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "server": { "type": "string" }
            }
        }),
        secrets: vec![],
        credentials: vec![],
        streaming: true,
        idempotent: false,
        output_mode: None,
        use_cases: vec![
            "trigger".into(),
            "http".into(),
            "webhook".into(),
            "server".into(),
        ],
        examples: vec![],
        see_also: vec!["schedule".into()],
    }
}

#[derive(Parser)]
#[command(
    name = "na-webhook",
    version,
    about = "HTTP webhook trigger for Ngalir flows",
    disable_version_flag = true
)]
struct Cli {
    #[arg(long, default_value = "8080")]
    port: u16,
    #[arg(long, default_value = "/")]
    path: String,
    #[arg(long)]
    flow: Option<String>,
    #[arg(long)]
    ngalir_bin: Option<String>,
    #[arg(long, default_value_t = 9091)]
    metrics_port: u16,
}

#[derive(Clone)]
struct AppState {
    flow_path: String,
    ngalir_bin: String,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--describe") {
        print_manifest(&manifest());
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let cli = Cli::parse_from(&args);

    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .try_init()
        .ok();

    let flow_path = match cli.flow {
        Some(f) => f,
        None => {
            eprintln!("--flow is required (pass a flow spec path)");
            std::process::exit(1);
        }
    };

    let ngalir_bin = cli.ngalir_bin.unwrap_or_else(|| "ngalir".to_string());

    let state = AppState {
        flow_path,
        ngalir_bin,
    };

    let app = Router::new()
        .route(&cli.path, post(handle_webhook))
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(Arc::new(state));

    let addr: SocketAddr = ([0, 0, 0, 0], cli.port).into();
    info!(port = cli.port, path = cli.path, "webhook server starting");

    let metrics_addr: SocketAddr = ([0, 0, 0, 0], cli.metrics_port).into();
    tokio::spawn(async move {
        let metrics_app = Router::new()
            .route("/health", get(health_handler))
            .route("/metrics", get(metrics_handler));
        let listener = tokio::net::TcpListener::bind(metrics_addr).await.unwrap();
        info!(metrics_port = cli.metrics_port, "metrics server starting");
        axum::serve(listener, metrics_app).await.unwrap();
    });

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_handler() -> StatusCode {
    StatusCode::OK
}

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&prometheus::gather(), &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap_or_default()
}

async fn handle_webhook(
    State(state): State<Arc<AppState>>,
    body: Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let input_json = serde_json::to_string(&body.0).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("JSON serialization error: {e}"),
        )
    })?;

    let child = Command::new(&state.ngalir_bin)
        .arg("run")
        .arg(&state.flow_path)
        .arg("--input")
        .arg(&input_json)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            FLOW_EXECUTIONS.with_label_values(&["spawn_failed"]).inc();
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to spawn ngalir: {e}"),
            )
        })?;

    let output = child.wait_with_output().await.map_err(|e| {
        FLOW_EXECUTIONS.with_label_values(&["wait_failed"]).inc();
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to wait for ngalir: {e}"),
        )
    })?;

    if output.status.success() {
        FLOW_EXECUTIONS.with_label_values(&["success"]).inc();
        let stdout = String::from_utf8_lossy(&output.stdout);
        match serde_json::from_str::<Value>(&stdout) {
            Ok(val) => Ok(Json(val)),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse ngalir output: {e}\nstdout: {stdout}"),
            )),
        }
    } else {
        FLOW_EXECUTIONS.with_label_values(&["failed"]).inc();
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(stderr = %stderr, "flow execution failed");
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Flow execution failed: {stderr}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-webhook");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(m.streaming);
        assert!(!m.idempotent);
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let bin = webhook_bin();
        let output = Command::new(&bin)
            .arg("--describe")
            .output()
            .expect("spawn na-webhook --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-webhook"));
        assert!(stdout.contains("\"streaming\": true"));
    }

    fn webhook_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-webhook");
        p
    }
}
