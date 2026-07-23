use na_contract::{
    exit_code, fail, print_manifest, read_input, AuthType, CredentialField, CredentialSpec,
    Manifest,
};
use serde_json::Value;

const TWILIO_API_BASE: &str = "https://api.twilio.com";

fn manifest() -> Manifest {
    Manifest {
        name: "na-twilio".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Send SMS and WhatsApp messages via Twilio API.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["send_sms", "send_whatsapp"], "default": "send_sms" },
                "from": { "type": "string", "description": "Twilio phone number (E.164 format, e.g. +1234567890)" },
                "to": { "type": "string", "description": "Recipient phone number (E.164 format)" },
                "body": { "type": "string", "description": "Message body text" }
            },
            "required": ["from", "to", "body"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" },
                "sid": { "type": "string", "description": "Twilio message SID" },
                "status": { "type": "string", "description": "Message status" }
            }
        }),
        secrets: vec!["account_sid".into(), "auth_token".into()],
        credentials: vec![CredentialSpec {
            id: "twilio_credentials".into(),
            label: "Twilio API Credentials".into(),
            auth_type: AuthType::Custom,
            fields: vec![
                CredentialField {
                    key: "account_sid".into(),
                    label: "Account SID".into(),
                    input_type: "text".into(),
                    required: true,
                },
                CredentialField {
                    key: "auth_token".into(),
                    label: "Auth Token".into(),
                    input_type: "password".into(),
                    required: true,
                },
            ],
            oauth: None,
        }],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec![
            "twilio".into(),
            "sms".into(),
            "whatsapp".into(),
            "notification".into(),
        ],
        examples: vec![],
        see_also: vec!["email".into()],
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
    let action = input["action"].as_str().unwrap_or("send_sms");
    let account_sid = match na_contract::read_secret("account_sid") {
        Some(s) => s,
        None => fail(
            exit_code::AUTH,
            "missing Twilio Account SID (set NGALIR_SECRET_ACCOUNT_SID)",
        ),
    };
    let auth_token = match na_contract::read_secret("auth_token") {
        Some(s) => s,
        None => fail(
            exit_code::AUTH,
            "missing Twilio Auth Token (set NGALIR_SECRET_AUTH_TOKEN)",
        ),
    };
    let from = input["from"].as_str().unwrap_or("");
    if from.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'from' phone number");
    }
    let to = input["to"].as_str().unwrap_or("");
    if to.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'to' phone number");
    }
    let body = input["body"].as_str().unwrap_or("");
    if body.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'body' message text");
    }

    let from_param = match action {
        "send_whatsapp" => format!("whatsapp:{from}"),
        _ => from.to_string(),
    };
    let to_param = match action {
        "send_whatsapp" => format!("whatsapp:{to}"),
        _ => to.to_string(),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    cmd_send_message(
        &client,
        TWILIO_API_BASE,
        &account_sid,
        &auth_token,
        &from_param,
        &to_param,
        body,
    )
    .await;
}

async fn cmd_send_message(
    client: &reqwest::Client,
    base_url: &str,
    account_sid: &str,
    auth_token: &str,
    from: &str,
    to: &str,
    message_body: &str,
) {
    let url = format!("{base_url}/2010-04-01/Accounts/{account_sid}/Messages.json");
    let params = [("From", from), ("To", to), ("Body", message_body)];

    let resp = client
        .post(&url)
        .basic_auth(account_sid, Some(auth_token))
        .form(&params)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status_code = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status_code >= 400 {
        let msg = body["message"].as_str().unwrap_or("unknown error");
        fail(
            exit_code::GENERIC,
            format!("Twilio API error ({}): {msg}", status_code),
        );
    }

    let sid = body["sid"].as_str().unwrap_or("").to_string();
    let status = body["status"].as_str().unwrap_or("").to_string();
    let output = serde_json::json!({
        "ok": true,
        "sid": sid,
        "status": status,
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
        assert_eq!(m.name, "na-twilio");
        assert!(!m.version.is_empty());
        assert_eq!(m.secrets, vec!["account_sid", "auth_token"]);
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].auth_type, AuthType::Custom);
    }

    #[test]
    fn test_describe_output() {
        use std::process::Command;
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-twilio");
        let output = Command::new(&p)
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-twilio"));
    }

    #[tokio::test]
    async fn test_send_sms_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/2010-04-01/Accounts/AC123/Messages.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sid": "SM123",
                "status": "sent"
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        cmd_send_message(
            &client,
            &mock_server.uri(),
            "AC123",
            "auth-token",
            "+1234567890",
            "+0987654321",
            "Hello",
        )
        .await;
    }

    #[tokio::test]
    async fn test_send_whatsapp_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/2010-04-01/Accounts/AC456/Messages.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sid": "SM456",
                "status": "sent"
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        cmd_send_message(
            &client,
            &mock_server.uri(),
            "AC456",
            "auth-token",
            "whatsapp:+1234567890",
            "whatsapp:+0987654321",
            "Hello from WhatsApp",
        )
        .await;
    }
}
