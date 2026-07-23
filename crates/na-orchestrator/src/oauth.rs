use anyhow::{bail, Result};
use axum::extract::State;
use na_contract::{Manifest, OAuthConfig};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};

use crate::flow;
use crate::AppState;

#[derive(Clone)]
pub(crate) struct PendingOAuth {
    pub(crate) spec_id: String,
    pub(crate) spec_label: String,
    pub(crate) oauth_config: OAuthConfig,
    pub(crate) _created_at: std::time::Instant,
}

pub(crate) type OAuthStore = Arc<std::sync::RwLock<HashMap<String, PendingOAuth>>>;

fn find_oauth_spec(spec_id: &str) -> Option<(String, String, OAuthConfig)> {
    let binaries = flow::scan_binaries();
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

fn describe_binary_sync(path: &str) -> Result<crate::flow::NodeBin> {
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
    Ok(crate::flow::NodeBin {
        binary: path.to_string(),
        manifest,
    })
}

/// Redirect the user to the OAuth provider's authorization page.
pub(crate) async fn api_oauth_authorize(
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
        _created_at: std::time::Instant::now(),
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
pub(crate) async fn api_oauth_callback(
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

    match crate::vault::call_vault_create(create_req).await {
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
