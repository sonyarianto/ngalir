use anyhow::{bail, Context, Result};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::flow;

pub(crate) async fn resolve_vault_refs(input: &mut Value) -> Result<()> {
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
    let vault_bin = flow::find_in_node_path("na-vault").unwrap_or_else(|| "na-vault".into());
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

pub(crate) async fn call_vault(
    mode: &str,
    id: Option<&str>,
    stdin_data: Option<Value>,
) -> Result<Value> {
    let vault_bin = flow::find_in_node_path("na-vault").unwrap_or_else(|| "na-vault".into());
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

pub(crate) async fn call_vault_list() -> Result<Vec<Value>> {
    let result = call_vault("--list", None, None).await?;
    serde_json::from_value(result).context("na-vault list: expected array")
}

pub(crate) async fn call_vault_get(id: &str) -> Result<Value> {
    call_vault("--get", Some(id), None).await
}

pub(crate) async fn call_vault_create(data: Value) -> Result<Value> {
    call_vault("--create", None, Some(data)).await
}

pub(crate) async fn call_vault_update(id: &str, data: Value) -> Result<Value> {
    call_vault("--update", Some(id), Some(data)).await
}

pub(crate) async fn call_vault_delete(id: &str) -> Result<Value> {
    call_vault("--delete", Some(id), None).await
}
