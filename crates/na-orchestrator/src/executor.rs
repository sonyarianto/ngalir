use anyhow::{bail, Context, Result};
use prometheus::{register_int_counter_vec, IntCounterVec};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, Semaphore};
use tracing::{error, info, info_span, warn, Instrument};

use crate::api::StepCommand;
use crate::flow::{self, NodeBin, NodeSpec};
use crate::history;
use crate::state;
use crate::vault;
use crate::FlowSpec;

#[derive(Clone)]
pub(crate) struct StepConfig {
    pub(crate) flow_id: String,
    pub(crate) step_tx: tokio::sync::broadcast::Sender<StepCommand>,
}

pub(crate) type EventFn = dyn Fn(&str, Option<&str>, Option<&Value>, Option<&str>) + Send + Sync;

static FLOW_EXECUTIONS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "ngalir_flow_executions_total",
        "Total flow executions",
        &["status"]
    )
    .expect("failed to register Prometheus counter")
});

static NODE_EXECUTIONS: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "ngalir_node_executions_total",
        "Total node executions",
        &["node_type", "status"]
    )
    .expect("failed to register Prometheus counter")
});

pub(crate) async fn call_node(binary: &str, input: &Value) -> Result<Value> {
    let mut cmd = Command::new(binary);
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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_flow(
    flow: &FlowSpec,
    node_bins: &HashMap<String, NodeBin>,
    store: &mut state::StateStore,
    initial_outputs: HashMap<String, Value>,
    on_event: Option<&EventFn>,
    step_cfg: Option<&StepConfig>,
    history: Option<Arc<history::HistoryDb>>,
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
                .filter(|(_, n)| flow::deps_satisfied(n, &guard))
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
            let mut input = flow::build_input(n, &guard);

            if let Some(cond) = &n.when {
                if !flow::eval_when(cond, &guard)? {
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

            flow::interpolate_json(&mut input, &guard);
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

    flow::validate_input(&input, &bin.manifest.inputs, &node.id)?;
    let mut input = input;
    vault::resolve_vault_refs(&mut input).await?;

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
            let stream: Vec<Value> = flow::read_stream_output(reader).await?;
            let mut stderr = String::new();
            if let Some(stderr_pipe) = child.stderr.take() {
                tokio::io::BufReader::new(stderr_pipe)
                    .read_to_string(&mut stderr)
                    .await?;
            }

            let status = child.wait().await?;
            if status.success() {
                let val = if bin.manifest.output_is_file() {
                    flow::resolve_file_output(json!({"stream": stream}))
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
                    flow::resolve_file_output(val)
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
