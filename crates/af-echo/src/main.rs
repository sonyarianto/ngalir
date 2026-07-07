//! Sample AxisFlow node implementing the Node Contract.
//!
//! Demonstrates `--describe`, `--version`, and stdin/stdout JSON execution.

use af_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;

fn manifest() -> Manifest {
    Manifest {
        name: "af-echo".to_string(),
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
