//! Ngalir HTTP client node.

use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-http".to_string(),
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
        credentials: vec![],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec!["http".into(), "api".into(), "webhook".into()],
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
    let body = na_contract::read_secret("body")
        .and_then(|s| serde_json::from_str(&s).ok())
        .or_else(|| input.get("body").cloned());
    if let Some(b) = body {
        if !b.is_null() {
            req = req.json(&b);
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

    let resp_text = resp.text().await.unwrap_or_default();
    let resp_body: Value = serde_json::from_str(&resp_text).unwrap_or(Value::String(resp_text));
    let output = serde_json::json!({
        "status": status,
        "headers": resp_headers,
        "body": resp_body,
    });
    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-http");
        assert!(m
            .inputs
            .get("required")
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("url")));
        assert_eq!(m.secrets, vec!["body"]);
        assert!(!m.idempotent);
    }
}
