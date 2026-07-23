use na_contract::{
    exit_code, fail, print_manifest, read_input, AuthType, CredentialField, CredentialSpec,
    Manifest, OAuthConfig,
};
use serde_json::Value;

const SLACK_API_BASE: &str = "https://slack.com/api";

fn manifest() -> Manifest {
    Manifest {
        name: "na-slack".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Slack messaging node: post messages and read channel history.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["post_message", "read_history"], "description": "Action to perform" },
                "channel": { "type": "string", "description": "Slack channel ID or name" },
                "text": { "type": "string", "description": "Message text (required for post_message)" },
                "count": { "type": "integer", "default": 10, "description": "Number of messages to retrieve (read_history only)" }
            },
            "required": ["action", "channel"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" },
                "ts": { "type": "string", "description": "Timestamp of posted message" },
                "messages": { "type": "array", "items": { "type": "object" } },
                "count": { "type": "integer" }
            }
        }),
        secrets: vec!["token".into()],
        credentials: vec![CredentialSpec {
            id: "slack_api".into(),
            label: "Slack API".into(),
            auth_type: AuthType::OAuth2,
            fields: vec![CredentialField {
                key: "access_token".into(),
                label: "Access Token".into(),
                input_type: "password".into(),
                required: true,
            }],
            oauth: Some(OAuthConfig {
                authorize_url: "https://slack.com/oauth/v2/authorize".into(),
                token_url: "https://slack.com/api/oauth.v2.access".into(),
                scopes: vec!["chat:write".into(), "channels:history".into()],
                client_id_env: "NGALIR_SLACK_CLIENT_ID".into(),
                client_secret_env: None,
            }),
        }],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec!["slack".into(), "messaging".into(), "chat".into()],
        examples: vec![],
        see_also: vec![],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--describe") {
        print_manifest(&manifest());
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(run());
}

async fn run() {
    let input = read_input();
    let action = input["action"].as_str().unwrap_or("");
    if action.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'action' field");
    }
    let channel = input["channel"].as_str().unwrap_or("");
    if channel.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'channel' field");
    }

    let token = na_contract::read_secret("token").unwrap_or_else(|| {
        fail(exit_code::AUTH, "missing Slack token (NGALIR_SECRET_TOKEN)");
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    match action {
        "post_message" => cmd_post_message(&client, &token, channel, SLACK_API_BASE, &input).await,
        "read_history" => cmd_read_history(&client, &token, channel, SLACK_API_BASE, &input).await,
        _ => fail(
            exit_code::INVALID_INPUT,
            format!("unknown action '{action}', expected 'post_message' or 'read_history'"),
        ),
    }
}

async fn cmd_post_message(
    client: &reqwest::Client,
    token: &str,
    channel: &str,
    base_url: &str,
    input: &Value,
) {
    let text = input["text"].as_str().unwrap_or("");
    if text.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'text' field for post_message action",
        );
    }

    let url = format!("{base_url}/chat.postMessage");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "channel": channel, "text": text }))
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("Slack API request failed: {e}")));

    let body: Value = resp.json().await.unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("failed to parse Slack response: {e}"),
        )
    });

    let ok = body.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        let error = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error");
        fail(exit_code::GENERIC, format!("Slack API error: {error}"));
    }

    let ts = body.get("ts").and_then(Value::as_str).unwrap_or("");
    let output = serde_json::json!({ "ok": true, "ts": ts });
    println!("{output}");
}

async fn cmd_read_history(
    client: &reqwest::Client,
    token: &str,
    channel: &str,
    base_url: &str,
    input: &Value,
) {
    let count = input.get("count").and_then(Value::as_u64).unwrap_or(10);
    let limit = count.to_string();

    let url = format!("{base_url}/conversations.history");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .query(&[("channel", channel), ("limit", &limit)])
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("Slack API request failed: {e}")));

    let body: Value = resp.json().await.unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("failed to parse Slack response: {e}"),
        )
    });

    let ok = body.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        let error = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown_error");
        fail(exit_code::GENERIC, format!("Slack API error: {error}"));
    }

    let messages = body
        .get("messages")
        .cloned()
        .unwrap_or(Value::Array(vec![]));
    let msg_count = messages.as_array().map(|a| a.len() as i64).unwrap_or(0);

    let output = serde_json::json!({ "messages": messages, "count": msg_count });
    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn bin_path() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-slack");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-slack");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(!m.streaming);
        assert!(!m.idempotent);
        assert!(m.inputs.get("required").is_some());
        assert!(m.secrets.contains(&"token".to_string()));
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].id, "slack_api");
        assert_eq!(m.credentials[0].auth_type, AuthType::OAuth2);
        assert!(m.credentials[0].oauth.is_some());
        let scopes = m.credentials[0].oauth.as_ref().unwrap().scopes.clone();
        assert!(scopes.contains(&"chat:write".to_string()));
        assert!(scopes.contains(&"channels:history".to_string()));
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(bin_path())
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-slack"));
        assert!(stdout.contains("slack_api"));
        assert!(stdout.contains("oauth2"));
    }

    #[test]
    fn test_missing_action_fails() {
        let input = serde_json::json!({"channel": "C123"});
        let mut child = Command::new(bin_path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(!output.status.success());
    }

    #[test]
    fn test_missing_channel_fails() {
        let input = serde_json::json!({"action": "post_message"});
        let mut child = Command::new(bin_path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.to_string().as_bytes())
                .unwrap();
        }
        let output = child.wait_with_output().expect("wait");
        assert!(!output.status.success());
    }

    // ── Mock HTTP tests ─────────────────────────────────────────────────────
    // Note: `fail()` calls process::exit(), so error paths must be tested via
    // subprocess (see test_missing_action_fails above), not via direct fn calls.

    #[tokio::test]
    async fn test_post_message_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"ok": true, "ts": "1234567890.123456"})),
            )
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"text": "Hello from test"});
        cmd_post_message(&client, "test-token", "C123", &mock_server.uri(), &input).await;
    }

    #[tokio::test]
    async fn test_read_history_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/conversations.history"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "messages": [
                    {"text": "first", "user": "U1", "ts": "111"},
                    {"text": "second", "user": "U2", "ts": "222"}
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"count": 2});
        cmd_read_history(&client, "test-token", "C123", &mock_server.uri(), &input).await;
    }
}
