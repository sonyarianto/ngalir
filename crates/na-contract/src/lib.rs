//! Shared Node Contract for Ngalir.
//!
//! Every `na-*` node implements this uniform interface:
//!   - `--describe`  -> prints the capability manifest as JSON
//!   - `--version`   -> prints the semver string
//!   - (default)     -> reads input JSON on stdin, writes output JSON on stdout
//!
//! Exit codes are standardized so the Orchestrator can decide retry/continue.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    ApiKey,
    BasicAuth,
    #[serde(rename = "oauth2")]
    OAuth2,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialField {
    pub key: String,
    pub label: String,
    #[serde(default = "default_cred_field_input_type")]
    pub input_type: String,
    #[serde(default)]
    pub required: bool,
}

fn default_cred_field_input_type() -> String {
    "text".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub authorize_url: String,
    pub token_url: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub client_id_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSpec {
    pub id: String,
    pub label: String,
    pub auth_type: AuthType,
    #[serde(default)]
    pub fields: Vec<CredentialField>,
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
}

/// A sample input/output pair for documentation / AI context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    pub input: Value,
    pub output: Value,
}

/// Capability manifest emitted by `na-* --describe`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    /// JSON Schema (draft 2020-12) describing the node's input object.
    #[serde(default = "Value::default")]
    pub inputs: Value,
    /// JSON Schema describing the node's output object.
    #[serde(default = "Value::default")]
    pub outputs: Value,
    /// Names of input fields that are credentials (resolved via `na-vault`).
    /// Legacy field — use `credentials` for richer credential specs.
    #[serde(default)]
    pub secrets: Vec<String>,
    /// Structured credential specs for UI forms, OAuth, and validation.
    #[serde(default)]
    pub credentials: Vec<CredentialSpec>,
    /// If true, stdout is NDJSON (one JSON object per line).
    #[serde(default)]
    pub streaming: bool,
    /// Hint: safe to retry on transient failure.
    #[serde(default)]
    pub idempotent: bool,
    /// Output transport: "stdout" (default, JSON via stdout) or "file" (write to NGALIR_OUTPUT_DIR, emit file path).
    #[serde(default)]
    pub output_mode: Option<String>,
    /// Tags describing use-case categories (e.g. ["csv", "etl", "import"]).
    #[serde(default)]
    pub use_cases: Vec<String>,
    /// Example input/output pairs for documentation and AI context.
    #[serde(default)]
    pub examples: Vec<Example>,
    /// Related node names (e.g. ["csv", "excel", "google-sheets"]).
    #[serde(default)]
    pub see_also: Vec<String>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: String::new(),
            description: String::new(),
            inputs: Value::Null,
            outputs: Value::Null,
            secrets: Vec::new(),
            credentials: Vec::new(),
            streaming: false,
            idempotent: false,
            output_mode: None,
            use_cases: Vec::new(),
            examples: Vec::new(),
            see_also: Vec::new(),
        }
    }
}

impl Manifest {
    /// Returns true if the node uses file-based output transport.
    pub fn output_is_file(&self) -> bool {
        self.output_mode.as_deref() == Some("file")
    }

    /// Returns credential specs for this node.
    ///
    /// If `credentials` is non-empty, those are returned.
    /// Otherwise, derives basic `AuthType::ApiKey` specs from the legacy
    /// `secrets` field for backward compatibility.
    pub fn credential_specs(&self) -> Vec<CredentialSpec> {
        if !self.credentials.is_empty() {
            return self.credentials.clone();
        }
        self.secrets
            .iter()
            .map(|s| CredentialSpec {
                id: s.clone(),
                label: s.clone(),
                auth_type: AuthType::ApiKey,
                fields: vec![CredentialField {
                    key: s.clone(),
                    label: s.clone(),
                    input_type: "password".to_string(),
                    required: true,
                }],
                oauth: None,
            })
            .collect()
    }
}

/// Standardized process exit codes.
pub mod exit_code {
    pub const SUCCESS: i32 = 0;
    pub const GENERIC: i32 = 1;
    pub const RETRYABLE: i32 = 2;
    pub const AUTH: i32 = 3;
    pub const INVALID_INPUT: i32 = 4;
}

/// Print a manifest as pretty JSON to stdout (used by `--describe`).
pub fn print_manifest(m: &Manifest) {
    println!(
        "{}",
        serde_json::to_string_pretty(m).expect("manifest serialize")
    );
}

/// Write a structured error to stderr and exit with the given code.
pub fn fail(code: i32, message: impl AsRef<str>) -> ! {
    let payload = serde_json::json!({
        "error": message.as_ref(),
        "code": code,
    });
    eprintln!("{}", payload);
    std::process::exit(code);
}

/// Read a secret from the environment (injected by the orchestrator).
///
/// Looks up `NGALIR_SECRET_<NAME>` (uppercased). Returns `None` when the
/// variable is absent, allowing the caller to fall through to a stdin value.
pub fn read_secret(name: &str) -> Option<String> {
    let key = format!("NGALIR_SECRET_{}", name.to_uppercase());
    std::env::var(key).ok()
}

/// Convenience: read the entire stdin as a JSON `Value`.
pub fn read_input() -> Value {
    use std::io::Read;
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        fail(exit_code::INVALID_INPUT, "failed to read stdin");
    }
    match serde_json::from_str(&buf) {
        Ok(v) => v,
        Err(e) => fail(exit_code::INVALID_INPUT, format!("invalid input JSON: {e}")),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serial_test::serial;

    #[test]
    fn test_manifest_roundtrip() {
        let m = Manifest {
            name: "na-test".into(),
            version: "1.2.3".into(),
            description: "test node".into(),
            inputs: json!({"type": "object", "properties": {"x": {"type": "integer"}}}),
            outputs: json!({"type": "string"}),
            secrets: vec!["password".into()],
            credentials: vec![],
            streaming: true,
            idempotent: false,
            output_mode: Some("stdout".into()),
            use_cases: vec!["test".into()],
            examples: vec![Example {
                input: json!({"x": 1}),
                output: json!({"result": 2}),
            }],
            see_also: vec![],
        };
        let json = serde_json::to_string_pretty(&m).unwrap();
        let m2: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m.name, m2.name);
        assert_eq!(m.version, m2.version);
        assert_eq!(m.secrets, m2.secrets);
        assert!(m2.streaming);
        assert!(!m2.idempotent);
    }

    #[test]
    fn test_manifest_defaults() {
        let m: Manifest =
            serde_json::from_str(r#"{"name":"na-x","version":"0.1.0","description":"d"}"#).unwrap();
        assert!(m.inputs.is_null() || m.inputs == Value::Null);
        assert!(m.outputs.is_null() || m.outputs == Value::Null);
        assert!(m.secrets.is_empty());
        assert!(!m.streaming);
        assert!(!m.idempotent);
        assert_eq!(m.output_mode, None);
    }

    #[test]
    fn test_exit_codes_are_distinct() {
        assert_ne!(exit_code::SUCCESS, exit_code::GENERIC);
        assert_ne!(exit_code::GENERIC, exit_code::RETRYABLE);
        assert_ne!(exit_code::RETRYABLE, exit_code::AUTH);
        assert_ne!(exit_code::AUTH, exit_code::INVALID_INPUT);
    }

    #[test]
    #[serial]
    fn test_read_secret_found() {
        unsafe {
            std::env::set_var("NGALIR_SECRET_CONNECTION", "postgres://user:pass@db");
        }
        assert_eq!(
            read_secret("connection").as_deref(),
            Some("postgres://user:pass@db")
        );
        unsafe {
            std::env::remove_var("NGALIR_SECRET_CONNECTION");
        }
    }

    #[test]
    #[serial]
    fn test_read_secret_missing() {
        unsafe {
            std::env::remove_var("NGALIR_SECRET_MISSING");
        }
        assert!(read_secret("missing").is_none());
    }

    #[test]
    #[serial]
    fn test_read_secret_empty() {
        unsafe {
            std::env::set_var("NGALIR_SECRET_EMPTY", "");
        }
        assert_eq!(read_secret("empty").as_deref(), Some(""));
        unsafe {
            std::env::remove_var("NGALIR_SECRET_EMPTY");
        }
    }

    #[test]
    fn test_auth_type_serialization() {
        let cases = vec![
            (AuthType::ApiKey, r#""api_key""#),
            (AuthType::BasicAuth, r#""basic_auth""#),
            (AuthType::OAuth2, r#""oauth2""#),
            (AuthType::Custom, r#""custom""#),
        ];
        for (variant, expected) in cases {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected);
            let back: AuthType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn test_credential_spec_roundtrip() {
        let spec = CredentialSpec {
            id: "slack_api".into(),
            label: "Slack API".into(),
            auth_type: AuthType::OAuth2,
            fields: vec![CredentialField {
                key: "client_id".into(),
                label: "Client ID".into(),
                input_type: "text".into(),
                required: true,
            }],
            oauth: Some(OAuthConfig {
                authorize_url: "https://slack.com/oauth/authorize".into(),
                token_url: "https://slack.com/api/oauth.token".into(),
                scopes: vec!["chat:write".into()],
                client_id_env: "NGALIR_SLACK_CLIENT_ID".into(),
            }),
        };
        let json = serde_json::to_string_pretty(&spec).unwrap();
        let back: CredentialSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "slack_api");
        assert_eq!(back.auth_type, AuthType::OAuth2);
        assert!(back.oauth.is_some());
        assert_eq!(back.oauth.as_ref().unwrap().scopes, vec!["chat:write"]);
    }

    #[test]
    fn test_credential_spec_default_input_type() {
        let json = r#"{"key":"api_key","label":"API Key","required":true}"#;
        let field: CredentialField = serde_json::from_str(json).unwrap();
        assert_eq!(field.input_type, "text");
    }

    #[test]
    fn test_manifest_with_credentials() {
        let m = Manifest {
            name: "na-slack".into(),
            version: "0.1.0".into(),
            description: "Slack node".into(),
            inputs: json!({}),
            outputs: json!({}),
            secrets: vec![],
            credentials: vec![CredentialSpec {
                id: "slack_api".into(),
                label: "Slack API".into(),
                auth_type: AuthType::OAuth2,
                fields: vec![],
                oauth: Some(OAuthConfig {
                    authorize_url: "https://slack.com/oauth/authorize".into(),
                    token_url: "https://slack.com/api/oauth.token".into(),
                    scopes: vec![],
                    client_id_env: "NGALIR_SLACK_CLIENT_ID".into(),
                }),
            }],
            streaming: false,
            idempotent: true,
            output_mode: None,
            use_cases: vec![],
            examples: vec![],
            see_also: vec![],
        };
        let json = serde_json::to_string_pretty(&m).unwrap();
        let m2: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m2.credentials.len(), 1);
        assert_eq!(m2.credentials[0].auth_type, AuthType::OAuth2);
        assert!(m2.credentials[0].oauth.is_some());
    }

    #[test]
    fn test_credential_specs_backward_compat_from_secrets() {
        let m = Manifest {
            name: "na-db".into(),
            version: "0.1.0".into(),
            description: "DB node".into(),
            inputs: json!({}),
            outputs: json!({}),
            secrets: vec!["connection".into(), "password".into()],
            credentials: vec![],
            streaming: false,
            idempotent: true,
            output_mode: None,
            use_cases: vec![],
            examples: vec![],
            see_also: vec![],
        };
        let specs = m.credential_specs();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].id, "connection");
        assert_eq!(specs[0].auth_type, AuthType::ApiKey);
        assert_eq!(specs[0].fields.len(), 1);
        assert_eq!(specs[0].fields[0].input_type, "password");
        assert!(specs[0].oauth.is_none());
    }

    #[test]
    fn test_credential_specs_credentials_take_precedence() {
        let m = Manifest {
            name: "na-slack".into(),
            version: "0.1.0".into(),
            description: "Slack node".into(),
            inputs: json!({}),
            outputs: json!({}),
            secrets: vec!["token".into()],
            credentials: vec![CredentialSpec {
                id: "slack_oauth".into(),
                label: "Slack OAuth".into(),
                auth_type: AuthType::OAuth2,
                fields: vec![],
                oauth: None,
            }],
            streaming: false,
            idempotent: true,
            output_mode: None,
            use_cases: vec![],
            examples: vec![],
            see_also: vec![],
        };
        let specs = m.credential_specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].id, "slack_oauth");
        assert_eq!(specs[0].auth_type, AuthType::OAuth2);
    }
}
