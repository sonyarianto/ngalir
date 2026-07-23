use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";

fn manifest() -> Manifest {
    Manifest {
        name: "na-telegram".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Telegram Bot node: send messages and get updates.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["send_message", "get_updates"] },
                "chat_id": { "type": "string" },
                "text": { "type": "string" },
                "parse_mode": { "type": "string", "enum": ["MarkdownV2", "HTML"] },
                "offset": { "type": "integer" },
                "limit": { "type": "integer", "default": 100 }
            },
            "required": ["action", "chat_id"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" },
                "message_id": { "type": "integer" },
                "updates": { "type": "array" },
                "count": { "type": "integer" }
            }
        }),
        secrets: vec!["token".into()],
        credentials: vec![na_contract::CredentialSpec {
            id: "telegram_bot_token".into(),
            label: "Telegram Bot Token".into(),
            auth_type: na_contract::AuthType::ApiKey,
            fields: vec![na_contract::CredentialField {
                key: "bot_token".into(),
                label: "Bot Token".into(),
                input_type: "password".into(),
                required: true,
            }],
            oauth: None,
        }],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec!["telegram".into(), "messaging".into(), "bot".into()],
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
    let chat_id = input["chat_id"].as_str().unwrap_or("");

    if action.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'action'");
    }
    if chat_id.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'chat_id'");
    }

    let token = na_contract::read_secret("token").unwrap_or_else(|| {
        fail(
            exit_code::AUTH,
            "missing Telegram bot token (secret 'token')",
        );
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    match action {
        "send_message" => send_message(&client, &token, TELEGRAM_API_BASE, &input, chat_id).await,
        "get_updates" => get_updates(&client, &token, TELEGRAM_API_BASE, &input).await,
        _ => fail(
            exit_code::INVALID_INPUT,
            format!("unknown action '{action}'"),
        ),
    }
}

async fn send_message(
    client: &reqwest::Client,
    token: &str,
    base_url: &str,
    input: &Value,
    chat_id: &str,
) {
    let text = input["text"].as_str().unwrap_or("");
    if text.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'text' for send_message");
    }

    let mut params = serde_json::Map::new();
    params.insert("chat_id".into(), Value::String(chat_id.to_string()));
    params.insert("text".into(), Value::String(text.to_string()));

    if let Some(parse_mode) = input["parse_mode"].as_str() {
        params.insert("parse_mode".into(), Value::String(parse_mode.to_string()));
    }

    let url = format!("{base_url}/bot{token}/sendMessage");
    let resp = client
        .post(&url)
        .json(&params)
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Telegram API request failed: {e}"),
            );
        });

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("failed to parse Telegram response: {e}"),
        );
    });

    if status != 200 || !body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let desc = body["description"].as_str().unwrap_or("unknown error");
        fail(exit_code::GENERIC, format!("Telegram API error: {desc}"));
    }

    let result = &body["result"];
    let message_id = result["message_id"].as_i64().unwrap_or(0);
    let output = serde_json::json!({
        "ok": true,
        "message_id": message_id,
    });
    println!("{output}");
}

async fn get_updates(client: &reqwest::Client, token: &str, base_url: &str, input: &Value) {
    let offset = input["offset"].as_i64();
    let limit = input["limit"].as_i64().unwrap_or(100);

    let mut params = serde_json::Map::new();
    params.insert("limit".into(), Value::Number(limit.into()));

    if let Some(off) = offset {
        params.insert("offset".into(), Value::Number(off.into()));
    }

    let url = format!("{base_url}/bot{token}/getUpdates");
    let resp = client
        .post(&url)
        .json(&params)
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Telegram API request failed: {e}"),
            );
        });

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or_else(|e| {
        fail(
            exit_code::GENERIC,
            format!("failed to parse Telegram response: {e}"),
        );
    });

    if status != 200 || !body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let desc = body["description"].as_str().unwrap_or("unknown error");
        fail(exit_code::GENERIC, format!("Telegram API error: {desc}"));
    }

    let updates = body["result"].as_array().cloned().unwrap_or_default();
    let count = updates.len() as i64;
    let output = serde_json::json!({
        "updates": updates,
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
        assert_eq!(m.name, "na-telegram");
        assert!(m
            .inputs
            .get("required")
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("action")));
        assert_eq!(m.secrets, vec!["token"]);
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].id, "telegram_bot_token");
        assert_eq!(m.credentials[0].fields.len(), 1);
        assert_eq!(m.credentials[0].fields[0].key, "bot_token");
        assert!(!m.idempotent);
    }

    #[tokio::test]
    async fn test_send_message_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bottest-token/sendMessage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": {"message_id": 42}
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"text": "Hello"});
        send_message(&client, "test-token", &mock_server.uri(), &input, "chat123").await;
    }

    #[tokio::test]
    async fn test_get_updates_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bottest-token/getUpdates"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": [{"update_id": 1, "message": {"text": "Hi"}}]
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"limit": 10});
        get_updates(&client, "test-token", &mock_server.uri(), &input).await;
    }
}
