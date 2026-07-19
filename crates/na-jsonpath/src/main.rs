//! Ngalir JSON path extractor / transform node.

use na_contract::{print_manifest, read_input, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-jsonpath".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Extract / transform JSON via simple path expressions (e.g. rows.0.name)."
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-jsonpath");
        assert!(m
            .inputs
            .get("required")
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("data")));
        assert!(m
            .inputs
            .get("required")
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("filter")));
        assert!(m.idempotent);
    }

    #[test]
    fn test_resolve_path_dot_returns_self() {
        let v = json!({"a": 1});
        assert_eq!(resolve_path(&v, "."), v);
        assert_eq!(resolve_path(&v, ""), v);
    }

    #[test]
    fn test_resolve_path_nested_object() {
        let v = json!({"a": {"b": {"c": 42}}});
        assert_eq!(resolve_path(&v, "a.b.c"), json!(42));
    }

    #[test]
    fn test_resolve_path_array_index() {
        let v = json!({"rows": [{"name": "alice"}, {"name": "bob"}]});
        assert_eq!(resolve_path(&v, "rows.0.name"), json!("alice"));
        assert_eq!(resolve_path(&v, "rows.1.name"), json!("bob"));
    }

    #[test]
    fn test_resolve_path_missing_returns_null() {
        let v = json!({"a": 1});
        assert_eq!(resolve_path(&v, "a.b.c"), Value::Null);
        assert_eq!(resolve_path(&v, "x.y"), Value::Null);
    }

    #[test]
    fn test_resolve_path_out_of_bounds() {
        let v = json!([1, 2, 3]);
        assert_eq!(resolve_path(&v, "5"), Value::Null);
        assert_eq!(resolve_path(&v, "0"), json!(1));
    }

    #[test]
    fn test_resolve_path_non_array_segment() {
        let v = json!({"a": {"b": 1}});
        assert_eq!(resolve_path(&v, "a.999"), Value::Null);
    }
}
