use na_contract::{
    exit_code, fail, print_manifest, read_input, AuthType, CredentialField, CredentialSpec,
    Manifest,
};
use serde_json::Value;

const NOTION_API_BASE: &str = "https://api.notion.com/v1";

fn manifest() -> Manifest {
    Manifest {
        name: "na-notion".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Query Notion databases, create/update pages, append blocks.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["query_database", "get_page", "create_page", "update_page", "append_block"] },
                "database_id": { "type": "string", "description": "Notion database ID (required for query_database)" },
                "page_id": { "type": "string", "description": "Notion page ID (required for get_page/update_page/append_block)" },
                "properties": { "type": "object", "description": "Page properties (required for create_page/update_page)" },
                "children": { "type": "array", "description": "Blocks to append (required for append_block)" },
                "filter": { "type": "object", "description": "Database query filter" },
                "sorts": { "type": "array", "description": "Database query sorts" },
                "page_size": { "type": "integer", "default": 100 }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "results": { "type": "array" },
                "page": { "type": "object" },
                "ok": { "type": "boolean" },
                "count": { "type": "integer" },
                "has_more": { "type": "boolean" }
            }
        }),
        secrets: vec!["token".into()],
        credentials: vec![CredentialSpec {
            id: "notion_token".into(),
            label: "Notion Integration Token".into(),
            auth_type: AuthType::ApiKey,
            fields: vec![CredentialField {
                key: "token".into(),
                label: "Internal Integration Secret".into(),
                input_type: "password".into(),
                required: true,
            }],
            oauth: None,
        }],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec!["notion".into(), "database".into(), "wiki".into()],
        examples: vec![],
        see_also: vec!["airtable".into(), "google-sheets".into()],
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--describe") {
        print_manifest(&manifest());
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    run().await;
}

async fn run() {
    let input = read_input();
    let action = input["action"].as_str().unwrap_or("");
    if action.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'action' field");
    }

    let token = match na_contract::read_secret("token") {
        Some(t) => t,
        None => fail(
            exit_code::AUTH,
            "missing Notion token (set NGALIR_SECRET_TOKEN)",
        ),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    match action {
        "query_database" => cmd_query_database(&client, NOTION_API_BASE, &token, &input).await,
        "get_page" => cmd_get_page(&client, NOTION_API_BASE, &token, &input).await,
        "create_page" => cmd_create_page(&client, NOTION_API_BASE, &token, &input).await,
        "update_page" => cmd_update_page(&client, NOTION_API_BASE, &token, &input).await,
        "append_block" => cmd_append_block(&client, NOTION_API_BASE, &token, &input).await,
        _ => fail(
            exit_code::INVALID_INPUT,
            format!("unknown action '{action}'"),
        ),
    }
}

fn notion_headers(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    headers.insert(
        "Notion-Version",
        reqwest::header::HeaderValue::from_static("2022-06-28"),
    );
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    headers
}

async fn notion_post(
    client: &reqwest::Client,
    _base_url: &str,
    url: &str,
    token: &str,
    body: &Value,
) -> Value {
    let resp = client
        .post(url)
        .headers(notion_headers(token))
        .json(body)
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Notion API request failed: {e}"),
            )
        });
    let status = resp.status().as_u16();
    let json: Value = resp.json().await.unwrap_or(Value::Null);
    if status >= 400 {
        let msg = json["message"].as_str().unwrap_or("unknown error");
        fail(
            exit_code::GENERIC,
            format!("Notion API error ({}): {msg}", status),
        );
    }
    json
}

async fn notion_get(client: &reqwest::Client, _base_url: &str, url: &str, token: &str) -> Value {
    let resp = client
        .get(url)
        .headers(notion_headers(token))
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Notion API request failed: {e}"),
            )
        });
    let status = resp.status().as_u16();
    let json: Value = resp.json().await.unwrap_or(Value::Null);
    if status >= 400 {
        let msg = json["message"].as_str().unwrap_or("unknown error");
        fail(
            exit_code::GENERIC,
            format!("Notion API error ({}): {msg}", status),
        );
    }
    json
}

async fn notion_patch(
    client: &reqwest::Client,
    _base_url: &str,
    url: &str,
    token: &str,
    body: &Value,
) -> Value {
    let resp = client
        .patch(url)
        .headers(notion_headers(token))
        .json(body)
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Notion API request failed: {e}"),
            )
        });
    let status = resp.status().as_u16();
    let json: Value = resp.json().await.unwrap_or(Value::Null);
    if status >= 400 {
        let msg = json["message"].as_str().unwrap_or("unknown error");
        fail(
            exit_code::GENERIC,
            format!("Notion API error ({}): {msg}", status),
        );
    }
    json
}

async fn cmd_query_database(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let database_id = input["database_id"].as_str().unwrap_or("");
    if database_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'database_id' for query_database",
        );
    }
    let page_size = input["page_size"].as_u64().unwrap_or(100);

    let url = format!("{base_url}/databases/{database_id}/query");
    let mut body = serde_json::json!({"page_size": page_size});
    if let Some(filter) = input.get("filter") {
        body["filter"] = filter.clone();
    }
    if let Some(sorts) = input.get("sorts") {
        body["sorts"] = sorts.clone();
    }

    let result = notion_post(client, base_url, &url, token, &body).await;
    let results = result["results"].as_array().cloned().unwrap_or_default();
    let has_more = result["has_more"].as_bool().unwrap_or(false);
    let output = serde_json::json!({
        "results": results,
        "has_more": has_more,
        "count": results.len(),
    });
    println!("{output}");
}

async fn cmd_get_page(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let page_id = input["page_id"].as_str().unwrap_or("");
    if page_id.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'page_id' for get_page");
    }
    let url = format!("{base_url}/pages/{page_id}");
    let result = notion_get(client, base_url, &url, token).await;
    let output = serde_json::json!({"page": result});
    println!("{output}");
}

async fn cmd_create_page(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let properties = input.get("properties").and_then(Value::as_object).cloned();
    let properties = match properties {
        Some(p) => p,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'properties' for create_page",
        ),
    };
    let parent = input
        .get("parent")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "database_id", "database_id": ""}));

    let mut body = serde_json::json!({
        "parent": parent,
        "properties": properties,
    });
    if let Some(children) = input.get("children") {
        body["children"] = children.clone();
    }

    let url = format!("{base_url}/pages");
    let result = notion_post(client, base_url, &url, token, &body).await;
    let output = serde_json::json!({"page": result, "ok": true});
    println!("{output}");
}

async fn cmd_update_page(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let page_id = input["page_id"].as_str().unwrap_or("");
    if page_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'page_id' for update_page",
        );
    }
    let properties = input.get("properties").and_then(Value::as_object).cloned();
    let properties = match properties {
        Some(p) => p,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'properties' for update_page",
        ),
    };

    let url = format!("{base_url}/pages/{page_id}");
    let body = serde_json::json!({"properties": properties});
    let result = notion_patch(client, base_url, &url, token, &body).await;
    let output = serde_json::json!({"page": result, "ok": true});
    println!("{output}");
}

async fn cmd_append_block(client: &reqwest::Client, base_url: &str, token: &str, input: &Value) {
    let block_id = input["page_id"].as_str().unwrap_or("");
    if block_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'page_id' (block parent) for append_block",
        );
    }
    let children = input.get("children").and_then(Value::as_array).cloned();
    let children = match children {
        Some(c) => c,
        None => fail(
            exit_code::INVALID_INPUT,
            "missing 'children' array for append_block",
        ),
    };

    let url = format!("{base_url}/blocks/{block_id}/children");
    let body = serde_json::json!({"children": children});
    let _ = notion_patch(client, base_url, &url, token, &body).await;
    let output = serde_json::json!({"ok": true});
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
        assert_eq!(m.name, "na-notion");
        assert!(!m.version.is_empty());
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].id, "notion_token");
    }

    #[test]
    fn test_describe_output() {
        use std::process::Command;
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-notion");
        let output = Command::new(&p)
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-notion"));
    }

    #[tokio::test]
    async fn test_query_database_success() {
        let mock_server = MockServer::start().await;
        let base_url = mock_server.uri();
        Mock::given(method("POST"))
            .and(path("/databases/db123/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{"id": "1"}, {"id": "2"}],
                "has_more": false
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"database_id": "db123"});
        cmd_query_database(&client, &base_url, "test-token", &input).await;
    }

    #[tokio::test]
    async fn test_get_page_success() {
        let mock_server = MockServer::start().await;
        let base_url = mock_server.uri();
        Mock::given(method("GET"))
            .and(path("/pages/page123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "page123",
                "properties": {"title": "Test"}
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"page_id": "page123"});
        cmd_get_page(&client, &base_url, "test-token", &input).await;
    }
}
