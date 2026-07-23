use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tracing::error;

use crate::oauth::OAuthStore;
use crate::FlowSpec;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FlowEvent {
    pub(crate) r#type: String,
    pub(crate) flow_id: String,
    pub(crate) node_id: Option<String>,
    pub(crate) output: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StepCommand {
    pub(crate) action: String,
    pub(crate) flow_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Snapshot {
    pub(crate) id: usize,
    pub(crate) timestamp: String,
    pub(crate) flow_name: String,
    pub(crate) flow_id: String,
    pub(crate) outputs: HashMap<String, Value>,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) tx: broadcast::Sender<FlowEvent>,
    pub(crate) step_tx: broadcast::Sender<StepCommand>,
    pub(crate) snapshots: Arc<Mutex<Vec<Snapshot>>>,
    pub(crate) flows_dir: PathBuf,
    pub(crate) oauth_store: OAuthStore,
    pub(crate) public_url: String,
    pub(crate) history_path: PathBuf,
}

// ── Run / Step ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct RunRequest {
    pub(crate) flow: FlowSpec,
    #[serde(default)]
    pub(crate) flow_id: String,
    #[serde(default)]
    pub(crate) step: bool,
}

#[derive(Serialize)]
pub(crate) struct RunResponse {
    pub(crate) flow_id: String,
}

pub(crate) async fn api_run(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<Json<RunResponse>, StatusCode> {
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

    Ok(Json(RunResponse { flow_id }))
}

async fn run_flow_with_events(
    flow: FlowSpec,
    flow_id: String,
    tx: broadcast::Sender<FlowEvent>,
    step_tx: broadcast::Sender<StepCommand>,
    step: bool,
    snapshots: Arc<Mutex<Vec<Snapshot>>>,
    flow_name: String,
) -> anyhow::Result<HashMap<String, Value>> {
    let node_bins = crate::preflight(&flow).await?;
    let mut store = crate::state::StateStore::disabled();
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

    let step_cfg = step.then(|| crate::executor::StepConfig {
        flow_id: flow_id.clone(),
        step_tx: step_tx.clone(),
    });

    let history = crate::HistoryDb::new(crate::history_db_path())
        .ok()
        .map(Arc::new);
    let result = crate::executor::execute_flow(
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

// ── Nodes / Skills ──────────────────────────────────────────────────────────

pub(crate) async fn api_nodes() -> Json<Vec<Value>> {
    let mut out = Vec::new();
    for name in crate::scan_binaries() {
        if let Ok(bin) = crate::describe_binary(&name).await {
            out.push(json!({
                "name": bin.manifest.name,
                "version": bin.manifest.version,
                "description": bin.manifest.description,
                "streaming": bin.manifest.streaming,
                "idempotent": bin.manifest.idempotent,
                "use_cases": bin.manifest.use_cases,
            }));
        }
    }
    Json(out)
}

pub(crate) async fn api_skills() -> Json<Vec<Value>> {
    let mut out = Vec::new();
    for name in crate::scan_binaries() {
        if let Ok(bin) = crate::describe_binary(&name).await {
            out.push(serde_json::to_value(&bin.manifest).unwrap_or_default());
        }
    }
    Json(out)
}

// ── Generate & Validate ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct GenerateRequest {
    pub(crate) prompt: String,
    #[serde(default)]
    pub(crate) model: Option<String>,
}

pub(crate) async fn api_generate(
    State(_state): State<AppState>,
    Json(req): Json<GenerateRequest>,
) -> Result<Json<Value>, StatusCode> {
    let registry = super::skills_registry_json()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

    let bin = crate::discover_node("llm")
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let result = super::call_node(&bin.binary, &llm_input)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
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

    Ok(Json(json!({
        "yaml": yaml,
        "raw": raw,
    })))
}

#[derive(Deserialize)]
pub(crate) struct ValidateRequest {
    pub(crate) flow: FlowSpec,
}

#[derive(Serialize)]
pub(crate) struct ValidateIssue {
    pub(crate) node_id: String,
    pub(crate) severity: String,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct ValidateResponse {
    pub(crate) valid: bool,
    pub(crate) issues: Vec<ValidateIssue>,
}

pub(crate) async fn api_validate(
    State(_state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Json<ValidateResponse> {
    let mut issues = Vec::new();

    match crate::check_cycles(&req.flow.nodes) {
        Ok(()) => {}
        Err(e) => issues.push(ValidateIssue {
            node_id: "flow".into(),
            severity: "error".into(),
            message: e.to_string(),
        }),
    }

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

    for node in &req.flow.nodes {
        for (key, refstr) in &node.inputs {
            let upstream = crate::upstream_of(refstr);
            if !req.flow.nodes.iter().any(|n| n.id == upstream) {
                issues.push(ValidateIssue {
                    node_id: node.id.clone(),
                    severity: "error".into(),
                    message: format!("input '{}' references unknown node '{}'", key, upstream),
                });
            }
        }
    }

    for node in &req.flow.nodes {
        if !node.use_.starts_with('@') {
            match crate::discover_node(&node.use_).await {
                Ok(_) => {}
                Err(_) => issues.push(ValidateIssue {
                    node_id: node.id.clone(),
                    severity: "warning".into(),
                    message: format!("node type 'na-{}' not found on PATH", node.use_),
                }),
            }
        }
    }

    Json(ValidateResponse {
        valid: issues.is_empty(),
        issues,
    })
}

// ── Snapshots ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct SnapshotsResponse {
    pub(crate) snapshots: Vec<Snapshot>,
}

pub(crate) async fn api_snapshots(State(state): State<AppState>) -> Json<SnapshotsResponse> {
    let list = state.snapshots.lock().unwrap_or_else(|e| e.into_inner());
    Json(SnapshotsResponse {
        snapshots: list.clone(),
    })
}

#[derive(Deserialize)]
pub(crate) struct DiffQuery {
    pub(crate) from: usize,
    pub(crate) to: usize,
}

#[derive(Serialize)]
pub(crate) struct DiffEntry {
    pub(crate) node_id: String,
    pub(crate) from: Option<Value>,
    pub(crate) to: Option<Value>,
    pub(crate) changed: bool,
}

#[derive(Serialize)]
pub(crate) struct DiffResponse {
    pub(crate) from: Snapshot,
    pub(crate) to: Snapshot,
    pub(crate) diffs: Vec<DiffEntry>,
}

pub(crate) async fn api_snapshots_diff(
    State(state): State<AppState>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<DiffResponse>, StatusCode> {
    let list = state.snapshots.lock().unwrap_or_else(|e| e.into_inner());
    let from = list.get(query.from).cloned().ok_or(StatusCode::NOT_FOUND)?;
    let to = list.get(query.to).cloned().ok_or(StatusCode::NOT_FOUND)?;

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

    Ok(Json(DiffResponse { from, to, diffs }))
}

// ── Flow CRUD ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct FlowsListResponse {
    pub(crate) flows: Vec<crate::FlowMeta>,
}

pub(crate) async fn api_flows_list(State(state): State<AppState>) -> Json<FlowsListResponse> {
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
                    flows.push(crate::FlowMeta {
                        name: name.to_string(),
                        modified,
                    });
                }
            }
        }
    }
    flows.sort_by(|a, b| b.modified.cmp(&a.modified));
    Json(FlowsListResponse { flows })
}

#[derive(Serialize)]
pub(crate) struct FlowSaveResponse {
    pub(crate) name: String,
}

pub(crate) async fn api_flows_save(
    State(state): State<AppState>,
    Json(flow): Json<FlowSpec>,
) -> Result<Json<FlowSaveResponse>, StatusCode> {
    let name = if flow.name.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        flow.name.clone()
    };
    let path = state.flows_dir.join(format!("{name}.json"));
    let content =
        serde_json::to_string_pretty(&flow).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    std::fs::write(&path, content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(FlowSaveResponse { name }))
}

pub(crate) async fn api_flows_get(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<FlowSpec>, StatusCode> {
    let path = state.flows_dir.join(format!("{name}.json"));
    let content = std::fs::read_to_string(&path).map_err(|_| StatusCode::NOT_FOUND)?;
    let flow: FlowSpec =
        serde_json::from_str(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(flow))
}

#[derive(Deserialize)]
pub(crate) struct FlowUpdate {
    pub(crate) flow: FlowSpec,
}

pub(crate) async fn api_flows_update(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<FlowUpdate>,
) -> Result<Json<FlowSaveResponse>, StatusCode> {
    let path = state.flows_dir.join(format!("{name}.json"));
    if !path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }
    let content =
        serde_json::to_string_pretty(&req.flow).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    std::fs::write(&path, content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(FlowSaveResponse { name }))
}

pub(crate) async fn api_flows_delete(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let path = state.flows_dir.join(format!("{name}.json"));
    if !path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }
    std::fs::remove_file(&path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({"deleted": name})))
}

// ── Credential API ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct CredentialListResponse {
    pub(crate) credentials: Vec<Value>,
}

pub(crate) async fn api_credentials_list() -> Result<Json<CredentialListResponse>, StatusCode> {
    match crate::call_vault_list().await {
        Ok(list) => Ok(Json(CredentialListResponse { credentials: list })),
        Err(e) => {
            error!("credential list failed: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub(crate) async fn api_credentials_get(Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    match crate::call_vault_get(&id).await {
        Ok(cred) => Ok(Json(cred)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("credential get failed: {e}");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct CreateCredentialRequest {
    pub(crate) credential_spec_id: String,
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) auth_type: String,
    #[serde(default)]
    pub(crate) data: serde_json::Map<String, Value>,
}

pub(crate) async fn api_credentials_create(
    Json(req): Json<CreateCredentialRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let data = json!({
        "credential_spec_id": req.credential_spec_id,
        "label": req.label,
        "auth_type": req.auth_type,
        "data": req.data,
    });

    match crate::call_vault_create(data).await {
        Ok(cred) => Ok((StatusCode::CREATED, Json(cred))),
        Err(e) => {
            error!("credential create failed: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct UpdateCredentialRequest {
    #[serde(default)]
    pub(crate) label: Option<String>,
    #[serde(default)]
    pub(crate) auth_type: Option<String>,
    #[serde(default)]
    pub(crate) data: Option<serde_json::Map<String, Value>>,
}

pub(crate) async fn api_credentials_update(
    Path(id): Path<String>,
    Json(req): Json<UpdateCredentialRequest>,
) -> Result<Json<Value>, StatusCode> {
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

    match crate::call_vault_update(&id, Value::Object(data)).await {
        Ok(cred) => Ok(Json(cred)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("credential update failed: {e}");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub(crate) async fn api_credentials_delete(
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    match crate::call_vault_delete(&id).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("credential delete failed: {e}");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

pub(crate) async fn api_credentials_test(
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let cred = match crate::call_vault_get(&id).await {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") {
                return Err(StatusCode::NOT_FOUND);
            }
            error!("credential get for test failed: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let credential_spec_id = cred["credential_spec_id"].as_str().unwrap_or("");

    let binaries = crate::scan_binaries();
    let mut test_bin: Option<String> = None;
    for name in &binaries {
        if let Ok(bin) = crate::describe_binary(name).await {
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
            return Ok(Json(json!({
                "ok": false,
                "message": format!("no node found for credential spec '{credential_spec_id}'")
            })));
        }
    };

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
            return Ok(Json(json!({
                "ok": false,
                "message": format!("cannot spawn {binary}: {e}")
            })));
        }
    };

    {
        let mut stdin = match child.stdin.take() {
            Some(s) => s,
            None => {
                return Ok(Json(json!({
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
            return Ok(Json(json!({
                "ok": false,
                "message": format!("test connection failed: {e}")
            })));
        }
    };

    match serde_json::from_slice::<Value>(&out.stdout) {
        Ok(result) => Ok(Json(result)),
        Err(_) => Ok(Json(json!({
            "ok": out.status.success(),
            "message": String::from_utf8_lossy(&out.stdout).trim().to_string(),
        }))),
    }
}

// ── History API ─────────────────────────────────────────────────────────────

pub(crate) async fn api_history_list(
    State(state): State<AppState>,
) -> Result<Json<Value>, StatusCode> {
    let db = crate::HistoryDb::new(state.history_path.clone()).map_err(|e| {
        error!(error = %e, "failed to open history db");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match db.list_runs() {
        Ok(runs) => Ok(Json(json!({"runs": runs}))),
        Err(e) => {
            error!(error = %e, "history list failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub(crate) async fn api_history_get(
    State(state): State<AppState>,
    Path(flow_id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let db = crate::HistoryDb::new(state.history_path.clone()).map_err(|e| {
        error!(error = %e, "failed to open history db");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match db.get_run(&flow_id) {
        Ok(Some(run)) => Ok(Json(run)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!(error = %e, "history get failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
