//! AxisFlow credential storage.
//!
//! Reads secrets from a JSON vault file (default `~/.axisflow/vault.json`
//! or `AXISFLOW_VAULT_FILE`). Resolves `vault://<key>` references.

use af_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;
use std::path::PathBuf;

fn manifest() -> Manifest {
    Manifest {
        name: "af-vault".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Resolves vault:// refs to secret values from a JSON key-value store."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": { "ref": { "type": "string", "description": "e.g. vault://db/prod" } },
            "required": ["ref"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": { "secret": { "type": "string" } }
        }),
        secrets: vec![],
        streaming: false,
        idempotent: true,
    }
}

fn vault_path() -> PathBuf {
    if let Ok(p) = std::env::var("AXISFLOW_VAULT_FILE") {
        return PathBuf::from(p);
    }
    let home = dirs_fallback();
    home.join(".axisflow").join("vault.json")
}

fn dirs_fallback() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| {
            std::env::var("USERPROFILE").or_else(|_| {
                std::env::var("HOMEDRIVE").map(|d| {
                    let h = std::env::var("HOMEPATH").unwrap_or_default();
                    format!("{d}{h}")
                })
            })
        })
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn load_vault() -> serde_json::Map<String, Value> {
    let path = vault_path();
    match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(Value::Object(obj)) => obj,
            Ok(_) => {
                eprintln!("af-vault: {} is not a JSON object", path.display());
                Default::default()
            }
            Err(e) => {
                eprintln!("af-vault: failed to parse {}: {e}", path.display());
                Default::default()
            }
        },
        Err(e) => {
            eprintln!(
                "af-vault: cannot read {} ({}): {}",
                path.display(),
                e.kind(),
                e
            );
            Default::default()
        }
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
    let ref_str = match input.get("ref").and_then(Value::as_str) {
        Some(r) => r,
        None => fail(exit_code::INVALID_INPUT, "missing 'ref' field"),
    };

    let key = ref_str.strip_prefix("vault://").unwrap_or(ref_str);

    let vault = load_vault();
    match vault.get(key) {
        Some(secret) => {
            println!("{}", serde_json::json!({ "secret": secret }));
        }
        None => {
            fail(
                exit_code::GENERIC,
                format!(
                    "secret not found for '{key}' (vault file: {})",
                    vault_path().display()
                ),
            );
        }
    }
}
