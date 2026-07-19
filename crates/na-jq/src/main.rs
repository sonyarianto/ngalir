//! Ngalir JSON path extractor / transform node.

use na_contract::{print_manifest, read_input, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-jq".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Extract a value from JSON via dot-path syntax (e.g. rows.0.name)."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "data": { "description": "The JSON value to query" },
                "filter": { "type": "string", "description": "dot-path, e.g. rows.0.name" }
            },
            "required": ["data", "filter"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": { "result": {} }
        }),
        secrets: vec![],
        streaming: false,
        idempotent: true,
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

    let input = read_input();
    let data = input.get("data").unwrap_or(&Value::Null);
    let filter = input.get("filter").and_then(Value::as_str).unwrap_or(".");

    let result = resolve_path(data, filter);
    println!("{}", serde_json::json!({"result": result}));
}

fn resolve_path(value: &Value, path: &str) -> Value {
    if path.is_empty() || path == "." {
        return value.clone();
    }
    let mut current = value.clone();
    for segment in path.split('.') {
        current = match current {
            Value::Object(ref obj) => obj.get(segment).cloned().unwrap_or(Value::Null),
            Value::Array(ref arr) => {
                if let Ok(idx) = segment.parse::<usize>() {
                    arr.get(idx).cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            }
            _ => Value::Null,
        };
    }
    current
}
