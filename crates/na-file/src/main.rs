//! Ngalir file I/O node.

use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};

fn manifest() -> Manifest {
    Manifest {
        name: "na-file".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Read from or write to local files.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["read", "write"] },
                "path": { "type": "string" },
                "content": { "type": "string", "description": "content to write (required for write)" }
            },
            "required": ["action", "path"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "bytes": { "type": "integer" }
            }
        }),
        secrets: vec![],
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

    let input = read_input();
    let action = input["action"].as_str().unwrap_or("");
    let path = input["path"].as_str().unwrap_or("");

    if path.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'path'");
    }

    match action {
        "read" => {
            let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("read failed: {e}"));
            });
            let bytes = content.len();
            let out = serde_json::json!({"content": content, "bytes": bytes});
            println!("{out}");
        }
        "write" => {
            let content = input["content"].as_str().unwrap_or("");
            if let Some(parent) = std::path::Path::new(path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(path, content).unwrap_or_else(|e| {
                fail(exit_code::GENERIC, format!("write failed: {e}"));
            });
            let out = serde_json::json!({"bytes": content.len()});
            println!("{out}");
        }
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read' or 'write'"),
    }
}
