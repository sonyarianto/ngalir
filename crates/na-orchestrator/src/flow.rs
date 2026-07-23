use anyhow::{bail, Context, Result};
use na_contract::{is_leap, now_iso8601, Manifest};
use rhai::{Engine, Scope};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tracing::warn;

pub(crate) async fn read_stream_output<R>(reader: R) -> Result<Vec<Value>>
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlowSpec {
    pub(crate) version: u32,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default = "default_concurrency")]
    pub(crate) concurrency: usize,
    pub(crate) nodes: Vec<NodeSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) notes: Vec<NoteSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NoteSpec {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) position: Position,
    #[serde(default)]
    pub(crate) width: f64,
    #[serde(default)]
    pub(crate) height: f64,
    #[serde(default)]
    pub(crate) color: String,
}

fn default_concurrency() -> usize {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NodeSpec {
    pub(crate) id: String,
    #[serde(rename = "use")]
    pub(crate) use_: String,
    #[serde(default)]
    pub(crate) with: Value,
    #[serde(default)]
    pub(crate) inputs: HashMap<String, String>,
    #[serde(default)]
    pub(crate) when: Option<String>,
    #[serde(default)]
    pub(crate) on_error: Option<String>,
    #[serde(default)]
    pub(crate) exit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) position: Option<Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Position {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

pub(crate) fn parse_flow(path: &str) -> Result<FlowSpec> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    serde_yaml::from_str(&raw).context("parse flow spec")
}

#[derive(Debug, Clone)]
pub(crate) struct NodeBin {
    pub(crate) binary: String,
    pub(crate) manifest: Manifest,
}

pub(crate) async fn preflight(flow: &FlowSpec) -> Result<HashMap<String, NodeBin>> {
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

pub(crate) async fn discover_node(use_name: &str) -> Result<NodeBin> {
    let binary = format!("na-{use_name}");
    if let Some(full) = find_in_node_path(&binary) {
        return describe_binary(&full).await;
    }
    describe_binary(&binary).await
}

pub(crate) fn find_in_node_path(name: &str) -> Option<String> {
    let node_path = std::env::var("NGALIR_NODE_PATH").ok()?;
    for dir in std::env::split_paths(&node_path) {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

pub(crate) async fn describe_binary(path: &str) -> Result<NodeBin> {
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

pub(crate) fn scan_binaries() -> Vec<String> {
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

pub(crate) fn expand_subflows(nodes: &[NodeSpec], base_dir: &Path) -> Result<Vec<NodeSpec>> {
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

pub(crate) fn check_cycles(nodes: &[NodeSpec]) -> Result<()> {
    let idx_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();

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

pub(crate) fn resolve_file_output(val: Value) -> Value {
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

pub(crate) fn deps_satisfied(node: &NodeSpec, outputs: &HashMap<String, Value>) -> bool {
    node.inputs
        .values()
        .all(|r| outputs.contains_key(upstream_of(r)))
        && node
            .when
            .as_deref()
            .map(|w| refs_in_str(w).iter().all(|&u| outputs.contains_key(u)))
            .unwrap_or(true)
}

pub(crate) fn refs_in_str(s: &str) -> Vec<&str> {
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

pub(crate) fn upstream_of(refstr: &str) -> &str {
    refstr.split('.').next().unwrap_or(refstr)
}

pub(crate) fn build_input(node: &NodeSpec, outputs: &HashMap<String, Value>) -> Value {
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

pub(crate) fn resolve_ref(refstr: &str, outputs: &HashMap<String, Value>) -> Value {
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

pub(crate) fn eval_when(condition: &str, outputs: &HashMap<String, Value>) -> Result<bool> {
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

pub(crate) fn interpolate_json(value: &mut Value, outputs: &HashMap<String, Value>) {
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

pub(crate) fn interpolate_str(s: &str, outputs: &HashMap<String, Value>) -> String {
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

pub(crate) fn validate_input(input: &Value, schema: &Value, node_id: &str) -> Result<()> {
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

pub(crate) fn chrono_now() -> String {
    now_iso8601()
}

pub(crate) fn parse_iso8601_ms(s: &str) -> Result<i64> {
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

pub(crate) fn days_since_epoch(y: i64, m: i64, d: i64) -> i64 {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlowMeta {
    pub(crate) name: String,
    pub(crate) modified: String,
}
