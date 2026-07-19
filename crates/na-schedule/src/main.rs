use axum::routing::get;
use axum::Router;
use chrono::Utc;
use clap::Parser;
use cron::Schedule;
use na_contract::{print_manifest, Manifest};
use prometheus::{register_int_counter_vec, Encoder, IntCounterVec, TextEncoder};
use std::net::SocketAddr;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::LazyLock;
use tokio::process::Command;
use tracing::{error, info};

static SCHEDULE_TRIGGERS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "na_schedule_triggers_total",
        "Total scheduled trigger firings",
        &["status"]
    )
    .unwrap()
});

fn manifest() -> Manifest {
    Manifest {
        name: "na-schedule".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Cron-like timer that executes a flow on a schedule.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "cron": { "type": "string", "description": "Cron expression (e.g. '0 * * * * *')" },
                "flow": { "type": "string" },
                "input": { "type": "object", "default": {} }
            },
            "required": ["cron", "flow"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "triggered": { "type": "integer" }
            }
        }),
        secrets: vec![],
        streaming: true,
        idempotent: false,
    }
}

#[derive(Parser)]
#[command(
    name = "na-schedule",
    version,
    about = "Cron-based flow scheduler for Ngalir",
    disable_version_flag = true
)]
struct Cli {
    #[arg(long)]
    cron: Option<String>,
    #[arg(long)]
    flow: Option<String>,
    #[arg(long)]
    ngalir_bin: Option<String>,
    #[arg(long, default_value = "{}")]
    input: String,
    #[arg(long, default_value_t = 9092)]
    metrics_port: u16,
}

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&prometheus::gather(), &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap_or_default()
}

async fn health_handler() -> &'static str {
    "OK"
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

    let metrics_addr: SocketAddr = ([0, 0, 0, 0], cli.metrics_port).into();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/health", get(health_handler))
            .route("/metrics", get(metrics_handler));
        let listener = tokio::net::TcpListener::bind(metrics_addr).await.unwrap();
        info!(metrics_port = cli.metrics_port, "metrics server starting");
        axum::serve(listener, app).await.unwrap();
    });

    let cron_expr = match cli.cron {
        Some(c) => c,
        None => {
            eprintln!("--cron is required (e.g. '0 * * * * *' for every minute)");
            std::process::exit(1);
        }
    };
    let flow_path = match cli.flow {
        Some(f) => f,
        None => {
            eprintln!("--flow is required (path to flow spec)");
            std::process::exit(1);
        }
    };
    let ngalir_bin = cli.ngalir_bin.unwrap_or_else(|| "ngalir".to_string());

    let schedule = match Schedule::from_str(&cron_expr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid cron expression '{cron_expr}': {e}");
            std::process::exit(1);
        }
    };

    info!(
        cron = cron_expr,
        flow = flow_path,
        "schedule daemon starting"
    );

    let mut _triggered: u64 = 0;

    loop {
        let now = Utc::now();
        let next = match schedule.upcoming(Utc).next() {
            Some(t) => t,
            None => {
                error!("no upcoming schedule time; cron may be in the past");
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
        };

        let delay = (next - now).to_std().unwrap_or(std::time::Duration::ZERO);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }

        info!("cron trigger firing");
        SCHEDULE_TRIGGERS.with_label_values(&["triggered"]).inc();
        _triggered += 1;

        let input_json = if cli.input == "{}" {
            serde_json::json!({"__trigger__": "schedule", "cron": cron_expr}).to_string()
        } else {
            cli.input.clone()
        };

        let child = Command::new(&ngalir_bin)
            .arg("run")
            .arg(&flow_path)
            .arg("--input")
            .arg(&input_json)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match child {
            Ok(c) => {
                let output = c.wait_with_output().await;
                match output {
                    Ok(out) if out.status.success() => {
                        info!("flow execution succeeded");
                        SCHEDULE_TRIGGERS.with_label_values(&["succeeded"]).inc();
                    }
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        error!(stderr = %stderr, "flow execution failed");
                        SCHEDULE_TRIGGERS.with_label_values(&["failed"]).inc();
                    }
                    Err(e) => {
                        error!(error = %e, "failed to wait for ngalir");
                        SCHEDULE_TRIGGERS.with_label_values(&["wait_failed"]).inc();
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "failed to spawn ngalir");
                SCHEDULE_TRIGGERS.with_label_values(&["spawn_failed"]).inc();
            }
        }
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
        assert_eq!(m.name, "na-schedule");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(m.streaming);
        assert!(!m.idempotent);
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let bin = schedule_bin();
        let output = Command::new(&bin)
            .arg("--describe")
            .output()
            .expect("spawn na-schedule --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-schedule"));
        assert!(stdout.contains("\"streaming\": true"));
    }

    #[test]
    fn test_version_output() {
        let bin = schedule_bin();
        let output = Command::new(&bin)
            .arg("--version")
            .output()
            .expect("spawn na-schedule --version");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.trim().is_empty());
    }

    fn schedule_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-schedule");
        p
    }
}
