//! Ngalir credential storage.
//!
//! Reads secrets from a JSON vault file (default `~/.ngalir/vault.json`
//! or `NGALIR_VAULT_FILE`). Resolves `vault://<key>` references.

use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;
use std::path::PathBuf;

fn manifest() -> Manifest {
    Manifest {
        name: "na-vault".to_string(),
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
        output_mode: None,
        use_cases: vec![
            "secret".into(),
            "credential".into(),
            "security".into(),
            "vault".into(),
        ],
        examples: vec![],
        see_also: vec![],
    }
}

fn vault_path() -> PathBuf {
    if let Ok(p) = std::env::var("NGALIR_VAULT_FILE") {
        return PathBuf::from(p);
    }
    let home = dirs_fallback();
    home.join(".ngalir").join("vault.json")
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
                eprintln!("na-vault: {} is not a JSON object", path.display());
                Default::default()
            }
            Err(e) => {
                eprintln!("na-vault: failed to parse {}: {e}", path.display());
                Default::default()
            }
        },
        Err(e) => {
            eprintln!(
                "na-vault: cannot read {} ({}): {}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-vault");
        let required = m.inputs.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("ref")));
        assert!(m.idempotent);
    }

    #[test]
    #[serial]
    fn test_vault_path_respects_env() {
        unsafe {
            std::env::set_var("NGALIR_VAULT_FILE", "/tmp/test-vault.json");
        }
        assert_eq!(vault_path(), PathBuf::from("/tmp/test-vault.json"));
        unsafe {
            std::env::remove_var("NGALIR_VAULT_FILE");
        }
    }

    #[test]
    #[serial]
    fn test_vault_path_default_uses_home() {
        unsafe {
            std::env::remove_var("NGALIR_VAULT_FILE");
        }
        let home = dirs_fallback();
        let expected = home.join(".ngalir").join("vault.json");
        assert_eq!(vault_path(), expected);
    }

    #[test]
    #[serial]
    fn test_load_vault_file_not_found() {
        unsafe {
            std::env::set_var("NGALIR_VAULT_FILE", "/tmp/nonexistent-vault-12345.json");
        }
        let vault = load_vault();
        assert!(vault.is_empty());
        unsafe {
            std::env::remove_var("NGALIR_VAULT_FILE");
        }
    }

    #[test]
    #[serial]
    fn test_load_vault_invalid_json() {
        let dir = std::env::temp_dir();
        let path = dir.join("test-vault-bad.json");
        std::fs::write(&path, "not json").unwrap();
        unsafe {
            std::env::set_var("NGALIR_VAULT_FILE", path.to_str().unwrap());
        }
        let vault = load_vault();
        assert!(vault.is_empty());
        std::fs::remove_file(&path).unwrap();
        unsafe {
            std::env::remove_var("NGALIR_VAULT_FILE");
        }
    }

    #[test]
    #[serial]
    fn test_load_vault_valid() {
        let dir = std::env::temp_dir();
        let path = dir.join("test-vault-good.json");
        let content = r#"{"db/prod": "postgres://user:pass@host/db", "api/key": "sk-12345"}"#;
        std::fs::write(&path, content).unwrap();
        unsafe {
            std::env::set_var("NGALIR_VAULT_FILE", path.to_str().unwrap());
        }
        let vault = load_vault();
        assert_eq!(vault.len(), 2);
        assert_eq!(
            vault.get("db/prod").unwrap(),
            "postgres://user:pass@host/db"
        );
        assert_eq!(vault.get("api/key").unwrap(), "sk-12345");
        std::fs::remove_file(&path).unwrap();
        unsafe {
            std::env::remove_var("NGALIR_VAULT_FILE");
        }
    }

    #[test]
    #[serial]
    fn test_load_vault_not_an_object() {
        let dir = std::env::temp_dir();
        let path = dir.join("test-vault-array.json");
        std::fs::write(&path, "[1,2,3]").unwrap();
        unsafe {
            std::env::set_var("NGALIR_VAULT_FILE", path.to_str().unwrap());
        }
        let vault = load_vault();
        assert!(vault.is_empty());
        std::fs::remove_file(&path).unwrap();
        unsafe {
            std::env::remove_var("NGALIR_VAULT_FILE");
        }
    }
}
