use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-llm".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "LLM chat completions via OpenAI / Anthropic compatible API.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "model": { "type": "string", "default": "gpt-4o", "description": "Model name (e.g. gpt-4o, claude-3-opus, gemini-pro)" },
                "messages": { "type": "array", "items": { "type": "object", "properties": {
                    "role": { "type": "string", "enum": ["system", "user", "assistant"] },
                    "content": { "type": "string" }
                }}, "description": "Chat messages array (OpenAI format)" },
                "prompt": { "type": "string", "description": "Shortcut: single user message (alternative to messages)" },
                "temperature": { "type": "number", "default": 1.0, "description": "Sampling temperature (0-2)" },
                "max_tokens": { "type": "integer", "default": 4096, "description": "Maximum tokens in response" },
                "api_key": { "type": "string", "description": "API key (or use NGALIR_SECRET_API_KEY)" },
                "api_base": { "type": "string", "default": "https://api.openai.com/v1", "description": "API base URL for compatible backends" }
            },
            "required": []
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "model": { "type": "string" },
                "usage": { "type": "object" }
            }
        }),
        secrets: vec!["api_key".into()],
        streaming: true,
        idempotent: true,
        output_mode: None,
        use_cases: vec![
            "ai".into(),
            "llm".into(),
            "chat".into(),
            "generation".into(),
        ],
        examples: vec![na_contract::Example {
            input: serde_json::json!({"model": "gpt-4o", "prompt": "Hello"}),
            output: serde_json::json!({"content": "Hi there!", "model": "gpt-4o", "usage": {"total_tokens": 10}}),
        }],
        see_also: vec![],
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
    stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
    model: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Choice {
    message: Option<Message>,
    delta: Option<Delta>,
    #[serde(rename = "finish_reason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Delta {
    content: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[allow(dead_code)]
struct Usage {
    #[serde(rename = "prompt_tokens")]
    prompt_tokens: Option<u64>,
    #[serde(rename = "completion_tokens")]
    completion_tokens: Option<u64>,
    #[serde(rename = "total_tokens")]
    total_tokens: Option<u64>,
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

    let input = read_input();

    let api_key = resolve_api_key(&input);
    let api_base = input["api_base"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("https://api.openai.com/v1");
    let model = input["model"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("gpt-4o");

    let messages = build_messages(&input);
    if messages.is_empty() {
        fail(exit_code::INVALID_INPUT, "no messages or prompt provided");
    }

    let temperature = input["temperature"].as_f64();
    let max_tokens = input["max_tokens"].as_u64();

    let rt = tokio::runtime::Runtime::new()
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("runtime init failed: {e}")));

    rt.block_on(async {
        cmd_chat(api_base, model, messages, temperature, max_tokens, &api_key).await;
    });
}

fn resolve_api_key(input: &Value) -> String {
    if let Some(secret) = na_contract::read_secret("api_key") {
        return secret;
    }
    input["api_key"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            fail(
                exit_code::INVALID_INPUT,
                "'api_key' is required (or set NGALIR_SECRET_API_KEY)",
            )
        })
}

fn build_messages(input: &Value) -> Vec<Message> {
    if let Some(msgs) = input["messages"].as_array() {
        if !msgs.is_empty() {
            return msgs
                .iter()
                .filter_map(|m| {
                    let role = m["role"].as_str()?;
                    let content = m["content"].as_str()?;
                    Some(Message {
                        role: role.to_string(),
                        content: content.to_string(),
                    })
                })
                .collect();
        }
    }
    if let Some(prompt) = input["prompt"].as_str().filter(|s| !s.is_empty()) {
        return vec![Message {
            role: "user".to_string(),
            content: prompt.to_string(),
        }];
    }
    vec![]
}

async fn cmd_chat(
    api_base: &str,
    model: &str,
    messages: Vec<Message>,
    temperature: Option<f64>,
    max_tokens: Option<u64>,
    api_key: &str,
) {
    let url = format!("{}/chat/completions", api_base.trim_end_matches('/'));

    let body = ChatRequest {
        model: model.to_string(),
        messages,
        temperature,
        max_tokens,
        stream: false,
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("API request failed: {e}")));

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        fail(exit_code::GENERIC, format!("API error ({status}): {text}"));
    }

    let cr: ChatResponse = resp
        .json()
        .await
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("parse response failed: {e}")));

    let content = cr
        .choices
        .first()
        .and_then(|c| c.message.as_ref())
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let out = serde_json::json!({
        "content": content,
        "model": cr.model.unwrap_or_else(|| model.to_string()),
        "usage": cr.usage,
    });
    println!("{out}");
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn bin_path() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-llm");
        p
    }

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-llm");
        assert!(!m.version.is_empty());
        assert!(m.streaming);
        assert!(m.idempotent);
        assert!(m.secrets.contains(&"api_key".to_string()));
    }

    #[test]
    fn test_describe_output() {
        let output = Command::new(bin_path())
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-llm"));
        assert!(stdout.contains("\"streaming\": true"));
    }

    #[test]
    fn test_build_messages_from_array() {
        let input = serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello!"}
            ]
        });
        let msgs = build_messages(&input);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].content, "Hello!");
    }

    #[test]
    fn test_build_messages_from_prompt() {
        let input = serde_json::json!({"prompt": "Hi there"});
        let msgs = build_messages(&input);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hi there");
    }

    #[test]
    fn test_build_messages_empty() {
        assert!(build_messages(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let bin = bin_path();
        let input = serde_json::json!({"prompt": "hello"});
        let mut child = Command::new(&bin)
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
    fn test_missing_messages_and_prompt_fails() {
        let bin = bin_path();
        let input = serde_json::json!({"api_key": "sk-test"});
        let mut child = Command::new(&bin)
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
    fn test_chat_request_serialization() {
        let req = ChatRequest {
            model: "gpt-4o".into(),
            messages: vec![Message {
                role: "user".into(),
                content: "Hi".into(),
            }],
            temperature: Some(0.7),
            max_tokens: Some(100),
            stream: false,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "gpt-4o");
        assert_eq!(json["messages"][0]["content"], "Hi");
        assert_eq!(json["temperature"], 0.7);
        assert_eq!(json["max_tokens"], 100);
        assert_eq!(json["stream"], false);
    }
}
