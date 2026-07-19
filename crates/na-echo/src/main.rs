//! Sample Ngalir node implementing the Node Contract.
//!
//! Demonstrates `--describe`, `--version`, and stdin/stdout JSON execution.

use na_contract::{exit_code, fail, print_manifest, read_input, Example, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "na-echo".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Echoes the input `message` field back as `echo`.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": { "message": { "type": "string" } },
            "required": ["message"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": { "echo": { "type": "string" } }
        }),
        secrets: vec![],
        streaming: false,
        idempotent: true,
        output_mode: None,
        use_cases: vec!["test".into(), "debug".into()],
        examples: vec![Example {
            input: serde_json::json!({"message": "hello"}),
            output: serde_json::json!({"echo": "hello"}),
        }],
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

    let input = read_input();
    let message = match input.get("message").and_then(Value::as_str) {
        Some(m) => m,
        None => fail(exit_code::INVALID_INPUT, "missing string field `message`"),
    };

    let output = serde_json::json!({ "echo": message });
    println!("{output}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-echo");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(m
            .inputs
            .get("required")
            .unwrap()
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("message")));
        assert!(m.secrets.is_empty());
        assert!(m.idempotent);
    }
}
