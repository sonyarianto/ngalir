use na_contract::{
    exit_code, fail, print_manifest, read_input, AuthType, CredentialField, CredentialSpec,
    Manifest,
};
use serde_json::Value;

const STRIPE_API_BASE: &str = "https://api.stripe.com/v1";

fn manifest() -> Manifest {
    Manifest {
        name: "na-stripe".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Stripe API node: list, create, and manage payments and customers."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list_customers", "create_customer", "list_payments", "create_payment", "retrieve_payment"],
                    "description": "Action to perform"
                },
                "email": { "type": "string", "description": "Customer email (create_customer)" },
                "name": { "type": "string", "description": "Customer name (create_customer)" },
                "description": { "type": "string", "description": "Customer description (create_customer)" },
                "customer_id": { "type": "string", "description": "Customer ID filter (list_payments)" },
                "payment_id": { "type": "string", "description": "Payment intent ID (retrieve_payment)" },
                "amount": { "type": "integer", "description": "Amount in cents (create_payment)" },
                "currency": { "type": "string", "default": "usd", "description": "Currency code (create_payment)" },
                "source": { "type": "string", "description": "Payment method ID (create_payment)" },
                "limit": { "type": "integer", "default": 10, "description": "Max results for list operations" }
            },
            "required": ["action"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "customers": { "type": "array", "items": { "type": "object" } },
                "payments": { "type": "array", "items": { "type": "object" } },
                "customer": { "type": "object" },
                "payment": { "type": "object" },
                "id": { "type": "string" },
                "status": { "type": "string" },
                "count": { "type": "integer" },
                "has_more": { "type": "boolean" }
            }
        }),
        secrets: vec!["secret_key".into()],
        credentials: vec![CredentialSpec {
            id: "stripe_secret_key".into(),
            label: "Stripe Secret Key".into(),
            auth_type: AuthType::ApiKey,
            fields: vec![CredentialField {
                key: "secret_key".into(),
                label: "Secret Key".into(),
                input_type: "password".into(),
                required: true,
            }],
            oauth: None,
        }],
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec!["stripe".into(), "payment".into(), "billing".into()],
        examples: vec![],
        see_also: vec![],
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

    let secret_key = match na_contract::read_secret("secret_key") {
        Some(k) => k,
        None => fail(
            exit_code::AUTH,
            "missing Stripe secret key (set NGALIR_SECRET_SECRET_KEY)",
        ),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();

    match action {
        "list_customers" => cmd_list_customers(&client, &secret_key, STRIPE_API_BASE, &input).await,
        "create_customer" => cmd_create_customer(&client, &secret_key, STRIPE_API_BASE, &input).await,
        "list_payments" => cmd_list_payments(&client, &secret_key, STRIPE_API_BASE, &input).await,
        "create_payment" => cmd_create_payment(&client, &secret_key, STRIPE_API_BASE, &input).await,
        "retrieve_payment" => cmd_retrieve_payment(&client, &secret_key, STRIPE_API_BASE, &input).await,
        _ => fail(
            exit_code::INVALID_INPUT,
            format!(
                "unknown action '{action}', expected 'list_customers', 'create_customer', 'list_payments', 'create_payment', or 'retrieve_payment'"
            ),
        ),
    }
}

async fn cmd_list_customers(
    client: &reqwest::Client,
    secret_key: &str,
    base_url: &str,
    input: &Value,
) {
    let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(10);

    let url = format!("{base_url}/customers");
    let resp = client
        .get(&url)
        .basic_auth(secret_key, Some(""))
        .query(&[("limit", limit.to_string())])
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Stripe API request failed: {e}"),
            )
        });

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Stripe API error ({}): {error}", status),
        );
    }

    let customers = body["data"].as_array().cloned().unwrap_or_default();
    let count = customers.len() as i64;
    let has_more = body["has_more"].as_bool().unwrap_or(false);
    let output =
        serde_json::json!({ "customers": customers, "count": count, "has_more": has_more });
    println!("{output}");
}

async fn cmd_create_customer(
    client: &reqwest::Client,
    secret_key: &str,
    base_url: &str,
    input: &Value,
) {
    let mut params: Vec<(&str, String)> = Vec::new();

    if let Some(email) = input["email"].as_str() {
        if !email.is_empty() {
            params.push(("email", email.to_string()));
        }
    }
    if let Some(name) = input["name"].as_str() {
        if !name.is_empty() {
            params.push(("name", name.to_string()));
        }
    }
    if let Some(desc) = input["description"].as_str() {
        if !desc.is_empty() {
            params.push(("description", desc.to_string()));
        }
    }

    let url = format!("{base_url}/customers");
    let resp = client
        .post(&url)
        .basic_auth(secret_key, Some(""))
        .form(&params)
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Stripe API request failed: {e}"),
            )
        });

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Stripe API error ({}): {error}", status),
        );
    }

    let id = body["id"].as_str().unwrap_or("").to_string();
    let output = serde_json::json!({ "customer": body, "id": id });
    println!("{output}");
}

async fn cmd_list_payments(
    client: &reqwest::Client,
    secret_key: &str,
    base_url: &str,
    input: &Value,
) {
    let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(10);
    let mut params: Vec<(&str, String)> = vec![("limit", limit.to_string())];

    if let Some(customer_id) = input["customer_id"].as_str() {
        if !customer_id.is_empty() {
            params.push(("customer", customer_id.to_string()));
        }
    }

    let url = format!("{base_url}/payment_intents");
    let resp = client
        .get(&url)
        .basic_auth(secret_key, Some(""))
        .query(&params)
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Stripe API request failed: {e}"),
            )
        });

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Stripe API error ({}): {error}", status),
        );
    }

    let payments = body["data"].as_array().cloned().unwrap_or_default();
    let count = payments.len() as i64;
    let has_more = body["has_more"].as_bool().unwrap_or(false);
    let output = serde_json::json!({ "payments": payments, "count": count, "has_more": has_more });
    println!("{output}");
}

async fn cmd_create_payment(
    client: &reqwest::Client,
    secret_key: &str,
    base_url: &str,
    input: &Value,
) {
    let amount = input.get("amount").and_then(Value::as_i64).unwrap_or(-1);
    if amount <= 0 {
        fail(
            exit_code::INVALID_INPUT,
            "missing or invalid 'amount' (positive integer in cents) for create_payment action",
        );
    }

    let currency = input
        .get("currency")
        .and_then(Value::as_str)
        .unwrap_or("usd");
    let mut params: Vec<(&str, String)> = vec![
        ("amount", amount.to_string()),
        ("currency", currency.to_string()),
    ];

    if let Some(source) = input["source"].as_str() {
        if !source.is_empty() {
            params.push(("payment_method", source.to_string()));
            params.push(("confirm", "true".to_string()));
        }
    }

    let url = format!("{base_url}/payment_intents");
    let resp = client
        .post(&url)
        .basic_auth(secret_key, Some(""))
        .form(&params)
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Stripe API request failed: {e}"),
            )
        });

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Stripe API error ({}): {error}", status),
        );
    }

    let id = body["id"].as_str().unwrap_or("").to_string();
    let status_str = body["status"].as_str().unwrap_or("").to_string();
    let output = serde_json::json!({ "payment": body, "id": id, "status": status_str });
    println!("{output}");
}

async fn cmd_retrieve_payment(
    client: &reqwest::Client,
    secret_key: &str,
    base_url: &str,
    input: &Value,
) {
    let payment_id = input["payment_id"].as_str().unwrap_or("");
    if payment_id.is_empty() {
        fail(
            exit_code::INVALID_INPUT,
            "missing 'payment_id' for retrieve_payment action",
        );
    }

    let url = format!("{base_url}/payment_intents/{payment_id}");

    let resp = client
        .get(&url)
        .basic_auth(secret_key, Some(""))
        .send()
        .await
        .unwrap_or_else(|e| {
            fail(
                exit_code::GENERIC,
                format!("Stripe API request failed: {e}"),
            )
        });

    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);

    if status >= 400 {
        let error = body["error"]["message"].as_str().unwrap_or("unknown");
        fail(
            exit_code::GENERIC,
            format!("Stripe API error ({}): {error}", status),
        );
    }

    let output = serde_json::json!({ "payment": body });
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
        assert_eq!(m.name, "na-stripe");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(!m.streaming);
        assert!(!m.idempotent);
        assert!(m.inputs.get("required").is_some());
        assert!(m.secrets.contains(&"secret_key".to_string()));
        assert_eq!(m.credentials.len(), 1);
        assert_eq!(m.credentials[0].id, "stripe_secret_key");
        assert_eq!(m.credentials[0].auth_type, AuthType::ApiKey);
        assert!(m.credentials[0].oauth.is_none());
    }

    #[test]
    fn test_describe_output() {
        use std::path::PathBuf;
        use std::process::Command;

        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-stripe");
        let output = Command::new(&p)
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-stripe"));
        assert!(stdout.contains("stripe_secret_key"));
        assert!(stdout.contains("api_key"));
    }

    // ── Mock HTTP tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_customers_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/customers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "cus_1", "email": "a@b.com"}, {"id": "cus_2", "email": "c@d.com"}],
                "has_more": false
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"limit": 2});
        cmd_list_customers(&client, "sk_test", &mock_server.uri(), &input).await;
    }

    #[tokio::test]
    async fn test_create_customer_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/customers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "cus_new",
                "email": "test@example.com",
                "name": "Test User"
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"email": "test@example.com", "name": "Test User"});
        cmd_create_customer(&client, "sk_test", &mock_server.uri(), &input).await;
    }

    #[tokio::test]
    async fn test_list_payments_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/payment_intents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "pi_1", "amount": 2000, "status": "succeeded"}],
                "has_more": false
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"limit": 1});
        cmd_list_payments(&client, "sk_test", &mock_server.uri(), &input).await;
    }

    #[tokio::test]
    async fn test_create_payment_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/payment_intents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "pi_new",
                "amount": 5000,
                "currency": "usd",
                "status": "succeeded"
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"amount": 5000, "currency": "usd"});
        cmd_create_payment(&client, "sk_test", &mock_server.uri(), &input).await;
    }

    #[tokio::test]
    async fn test_retrieve_payment_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/payment_intents/pi_abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "pi_abc",
                "amount": 1500,
                "status": "succeeded"
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let input = serde_json::json!({"payment_id": "pi_abc"});
        cmd_retrieve_payment(&client, "sk_test", &mock_server.uri(), &input).await;
    }
}
