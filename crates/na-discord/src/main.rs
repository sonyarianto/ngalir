use na_contract::{
    exit_code, fail, print_manifest, read_input, AuthType, CredentialField, CredentialSpec,
    Manifest,
};
use serde_json::Value;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

fn manifest() -> Manifest {
    Manifest {
        name: "na-discord".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Send messages to Discord via webhook or bot token.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["send_webhook", "send_bot", "get_messages"] },
                "webhook_url": { "type": "string", "description": "Discord webhook URL (required for send_webhook)" },
                "channel_id": { "type": "string", "description": "Discord channel ID (required for send_bot/get_messages)" },
                "content": { "type": "string", "description": "Message content" },
                "username": { "type": "string", "description": "Override username (webhook only)" },
                "avatar_url": { "type": "string", "description": "Override avatar URL (webhook only)" },
                "limit": { "type": "integer", "default": 50, "description": "Message limit for get_messages" }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" },
                "message_id": { "type": "string" },
                "messages": { "type": "array" },
                "count": { "type": "integer" }
            }
        }),
        secrets: vec!["token".into()],
        credentials: vec![CredentialSpec {
            id: "discord_bot_token".into(),
            label: "Discord Bot Token".into(),
            auth_type: AuthType::ApiKey,
            fields: vec![CredentialField {
                key: "token".into(),
                label: "Bot Token".into(),
                input_type: "password".into(),
                required: true,
            }],
            oauth: None,
        }],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec!["discord".into(), "chat".into(), "notification".into()],
        examples: vec![],
        see_also: vec!["slack".into(), "telegram".into()],
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

    let token = na_contract::read_secret("token").unwrap_or_else(|| {
        fail(
            exit_code::AUTH,
            "missing bot token (set NGALIR_SECRET_TOKEN)",
        );
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    match action {
        "send_webhook" => cmd_send_webhook(&client, &input).await,
        "send_bot" => cmd_send_bot(&client, DISCORD_API_BASE, &token, &input).await,
        "get_messages" => cmd_get_messages(&client, DISCORD_API_BASE, &token, &input).await,
        _ => fail(
            exit_code::INVALID_INPUT,
            format!("unknown action '{}'", action),
        ),
    }
}

async fn cmd_send_webhook(client: &reqwest::Client, input: &Value) {
    let webhook_url = input["webhook_url"].as_str().unwrap_or("");
    if webhook_url.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'webhook_url' for send_webhook",
        );
    }
    let content = input["content"].as_str().unwrap_or("");
    if content.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'content' for send_webhook",
        );
    }

    let mut payload = serde_json::json!({"content": content});
    if let Some(username) = input["username"].as_str() {
        payload["username"] = Value::String(username.to_string());
    }
    if let Some(avatar_url) = input["avatar_url"].as_str() {
        payload["avatar_url"] = Value::String(avatar_url.to_string());
    }

    let resp = client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("webhook request failed: {e}")));

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("webhook error: {body}"));
    }

    let output = serde_json::json!({"ok": true});
    println!("{output}");
}

async fn cmd_send_bot(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let channel_id = input["channel_id"].as_str().unwrap_or("");
    if channel_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'channel_id' for send_bot",
        );
    }
    let content = input["content"].as_str().unwrap_or("");
    if content.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'content' for send_bot");
    }

    let payload = serde_json::json!({
        "content": content,
    });

    let url = format!("{base_url}/channels/{channel_id}/messages");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bot {token}"))
        .json(&payload)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status().as_u16();
    let body: Value = resp
        .json()
        .await
        .unwrap_or_else(|_| Value::String(String::new()));

    if status >= 400 {
        fail(
            exit_code::GENERIC,
            format!("Discord API error ({}): {}", status, body),
        );
    }

    let message_id = body["id"].as_str().unwrap_or("").to_string();
    let output = serde_json::json!({
        "ok": true,
        "message_id": message_id,
    });
    println!("{output}");
}

async fn cmd_get_messages(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let channel_id = input["channel_id"].as_str().unwrap_or("");
    if channel_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'channel_id' for get_messages",
        );
    }

    let limit = input["limit"].as_u64().unwrap_or(50);

    let url = format!("{base_url}/channels/{channel_id}/messages?limit={limit}");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bot {token}"))
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        fail(
            exit_code::GENERIC,
            format!("Discord API error ({}): {}", status, body),
        );
    }

    let messages: Value = resp.json().await.unwrap_or_else(|_| Value::Array(vec![]));
    let count = messages.as_array().map(|a| a.len()).unwrap_or(0);

    let output = serde_json::json!({
        "messages": messages,
        "count": count,
    });
    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-discord");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(m.inputs.get("required").is_some());
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].id, "discord_bot_token");
    }

    #[test]
    fn test_describe_output() {
        use std::process::Command;
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-discord");
        let output = Command::new(&p)
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-discord"));
    }

    #[tokio::test]
    async fn test_send_bot_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/123/messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "456"})),
            )
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"channel_id": "123", "content": "Hello"});
        cmd_send_bot(&client, &mock_server.uri(), "test-token", &input).await;
    }

    #[tokio::test]
    async fn test_get_messages_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/123/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([{"id": "1"}, {"id": "2"}])),
            )
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"channel_id": "123", "limit": 10});
        cmd_get_messages(&client, &mock_server.uri(), "test-token", &input).await;
    }
}
