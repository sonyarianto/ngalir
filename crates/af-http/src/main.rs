//! AxisFlow HTTP client node.

use af_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "af-http".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Make HTTP requests (GET / POST / PUT / DELETE / PATCH).".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "method": { "type": "string", "default": "GET", "enum": ["GET","POST","PUT","DELETE","PATCH"] },
                "url": { "type": "string", "format": "uri" },
                "headers": { "type": "object" },
                "body": {}
            },
            "required": ["url"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "integer" },
                "headers": { "type": "object" },
                "body": {}
            }
        }),
        secrets: vec!["body".into()],
        streaming: false,
        idempotent: false,
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
    let url = input["url"].as_str().unwrap_or("");
    if url.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'url'");
    }
    let method = input["method"].as_str().unwrap_or("GET").to_uppercase();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let mut req = match method.as_str() {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => client.get(url),
    };

    if let Some(headers) = input["headers"].as_object() {
        for (k, v) in headers {
            if let Some(s) = v.as_str() {
                req = req.header(k.as_str(), s);
            }
        }
    }
    if let Some(body) = input.get("body") {
        if !body.is_null() {
            req = req.json(body);
        }
    }

    let resp = req.send().await.unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("HTTP request failed: {e}"));
    });
    let status = resp.status().as_u16() as i32;
    let resp_headers: Value = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                Value::String(v.to_str().unwrap_or("").into()),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into();

    let resp_body: Value = resp.json().await.unwrap_or(Value::Null);
    let output = serde_json::json!({
        "status": status,
        "headers": resp_headers,
        "body": resp_body,
    });
    println!("{output}");
}
