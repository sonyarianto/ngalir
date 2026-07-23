use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use std::path::PathBuf;

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
        credentials: vec![],
        streaming: false,
        idempotent: false,
        output_mode: Some("file".into()),
        use_cases: vec!["file".into(), "io".into(), "storage".into()],
        examples: vec![],
        see_also: vec!["csv".into(), "excel".into()],
    }
}

fn output_file_path() -> Option<PathBuf> {
    std::env::var("NGALIR_OUTPUT_DIR")
        .ok()
        .map(|d| PathBuf::from(d).join("output.json"))
}

fn write_output(val: serde_json::Value) {
    if let Some(out_path) = output_file_path() {
        let json = serde_json::to_string(&val).expect("serialize");
        std::fs::write(&out_path, &json).unwrap_or_else(|e| {
            fail(exit_code::GENERIC, format!("write output file failed: {e}"));
        });
        println!("\"{}\"", out_path.display());
    } else {
        println!("{val}");
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
            write_output(out);
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
            write_output(out);
        }
        _ => fail(exit_code::INVALID_INPUT, "action must be 'read' or 'write'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-file");
        let required = m.inputs.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")));
        assert!(required.contains(&serde_json::json!("path")));
        assert!(m.secrets.is_empty());
        assert_eq!(m.output_mode, Some("file".into()));
    }

    #[test]
    fn test_manifest_has_read_write_actions() {
        let m = manifest();
        let actions = m.inputs["properties"]["action"]["enum"].as_array().unwrap();
        let vals: Vec<&str> = actions.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(vals.contains(&"read"));
        assert!(vals.contains(&"write"));
    }
}
