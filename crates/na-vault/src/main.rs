use na_contract::{exit_code, fail, now_iso8601, print_manifest, read_input, Manifest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

// ── Manifest ─────────────────────────────────────────────────────────────────

fn manifest() -> Manifest {
    Manifest {
        name: "na-vault".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Structured credential store with at-rest encryption. Resolves vault:// refs, manages credential CRUD."
            .to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "ref": { "type": "string", "description": "vault://<credential_id> ref to resolve" }
            },
            "required": ["ref"]
        }),
        outputs: serde_json::json!({}),
        secrets: vec![],
        credentials: vec![],
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

// ── Data Structures ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultFile {
    version: u32,
    credentials: Vec<Credential>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Credential {
    id: String,
    credential_spec_id: String,
    label: String,
    auth_type: String,
    data: serde_json::Map<String, Value>,
    created_at: String,
    updated_at: String,
}

// ── Vault Path ───────────────────────────────────────────────────────────────

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

// ── Encryption ───────────────────────────────────────────────────────────────

fn encryption_key() -> Option<[u8; 32]> {
    let key_b64 = std::env::var("NGALIR_VAULT_KEY").ok()?;
    let raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &key_b64).ok()?;
    if raw.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&raw);
    Some(arr)
}

fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    use rand::RngCore;
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext).expect("encryption failed");
    let mut out = nonce_bytes.to_vec();
    out.extend(ciphertext);
    out
}

fn decrypt(data: &[u8], key: &[u8; 32]) -> Option<Vec<u8>> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
    if data.len() < 12 {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key).expect("valid key");
    let nonce = Nonce::from_slice(&data[..12]);
    cipher.decrypt(nonce, &data[12..]).ok()
}

// ── Vault I/O ────────────────────────────────────────────────────────────────

fn active_vault() -> VaultFile {
    let path = vault_path();
    let raw = match std::fs::read(&path) {
        Ok(d) => d,
        Err(_) => {
            return VaultFile {
                version: 1,
                credentials: vec![],
            }
        }
    };

    let decrypted = match encryption_key() {
        Some(key) => {
            match decrypt(&raw, &key) {
                Some(d) => d,
                None => {
                    // Could be legacy plain JSON
                    raw.clone()
                }
            }
        }
        None => raw,
    };

    match serde_json::from_slice::<VaultFile>(&decrypted) {
        Ok(vf) => vf,
        Err(_) => {
            // Try legacy flat format: { "key": "value", ... }
            if let Ok(Value::Object(map)) = serde_json::from_slice(&decrypted) {
                return migrate_legacy(map);
            }
            eprintln!("na-vault: invalid vault file at {}", path.display());
            VaultFile {
                version: 1,
                credentials: vec![],
            }
        }
    }
}

fn save_vault(vf: &VaultFile) {
    let path = vault_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let json = serde_json::to_string_pretty(vf)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("serialize vault failed: {e}")));

    let data = match encryption_key() {
        Some(key) => encrypt(json.as_bytes(), &key),
        None => json.into_bytes(),
    };

    std::fs::write(&path, &data)
        .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("failed to write vault: {e}")));
}

fn migrate_legacy(map: serde_json::Map<String, Value>) -> VaultFile {
    let mut creds = Vec::new();
    for (key, value) in map {
        let now = chrono_now();
        creds.push(Credential {
            id: key.clone(),
            credential_spec_id: "legacy".into(),
            label: key.clone(),
            auth_type: "api_key".into(),
            data: {
                let mut m = serde_json::Map::new();
                m.insert("value".into(), value);
                m
            },
            created_at: now.clone(),
            updated_at: now,
        });
    }
    VaultFile {
        version: 1,
        credentials: creds,
    }
}

fn chrono_now() -> String {
    now_iso8601()
}

fn generate_id() -> String {
    format!(
        "cred_{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("0000")
    )
}

// ── CRUD Operations ──────────────────────────────────────────────────────────

fn list_credentials() -> Vec<Value> {
    let vault = active_vault();
    vault
        .credentials
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "credential_spec_id": c.credential_spec_id,
                "label": c.label,
                "auth_type": c.auth_type,
                "created_at": c.created_at,
                "updated_at": c.updated_at,
            })
        })
        .collect()
}

fn get_credential(id: &str) -> Option<Credential> {
    let vault = active_vault();
    vault.credentials.into_iter().find(|c| c.id == id)
}

fn create_credential(input: &Value) -> Credential {
    let credential_spec_id = input
        .get("credential_spec_id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "missing 'credential_spec_id'"));
    let label = input
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "missing 'label'"));
    let auth_type = input
        .get("auth_type")
        .and_then(Value::as_str)
        .unwrap_or("api_key");
    let data = input
        .get("data")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let now = chrono_now();
    Credential {
        id: generate_id(),
        credential_spec_id: credential_spec_id.to_string(),
        label: label.to_string(),
        auth_type: auth_type.to_string(),
        data,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn save_credential(cred: &Credential) {
    let mut vault = active_vault();
    vault.credentials.push(cred.clone());
    save_vault(&vault);
}

fn update_credential(id: &str, input: &Value) -> Option<Credential> {
    let mut vault = active_vault();
    let idx = vault.credentials.iter().position(|c| c.id == id)?;
    {
        let cred = &mut vault.credentials[idx];
        if let Some(label) = input.get("label").and_then(Value::as_str) {
            cred.label = label.to_string();
        }
        if let Some(auth_type) = input.get("auth_type").and_then(Value::as_str) {
            cred.auth_type = auth_type.to_string();
        }
        if let Some(data) = input.get("data").and_then(Value::as_object) {
            cred.data = data.clone();
        }
        cred.updated_at = chrono_now();
    }
    let result = vault.credentials[idx].clone();
    save_vault(&vault);
    Some(result)
}

fn delete_credential(id: &str) -> bool {
    let mut vault = active_vault();
    let len_before = vault.credentials.len();
    vault.credentials.retain(|c| c.id != id);
    if vault.credentials.len() == len_before {
        return false;
    }
    save_vault(&vault);
    true
}

fn cmd_list() {
    println!(
        "{}",
        serde_json::to_string_pretty(&list_credentials())
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("serialize failed: {e}")))
    );
}

fn cmd_get(id: &str) {
    match get_credential(id) {
        Some(cred) => println!(
            "{}",
            serde_json::to_string_pretty(&cred)
                .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("serialize failed: {e}")))
        ),
        None => fail(exit_code::GENERIC, format!("credential '{id}' not found")),
    }
}

fn cmd_create() {
    let input = read_input();
    let cred = create_credential(&input);
    save_credential(&cred);
    println!(
        "{}",
        serde_json::to_string_pretty(&cred)
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("serialize failed: {e}")))
    );
}

fn cmd_update(id: &str) {
    let input = read_input();
    match update_credential(id, &input) {
        Some(cred) => println!(
            "{}",
            serde_json::to_string_pretty(&cred)
                .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("serialize failed: {e}")))
        ),
        None => fail(exit_code::GENERIC, format!("credential '{id}' not found")),
    }
}

fn cmd_delete(id: &str) {
    if delete_credential(id) {
        println!("{}", serde_json::json!({"ok": true}));
    } else {
        fail(exit_code::GENERIC, format!("credential '{id}' not found"));
    }
}

fn cmd_resolve() {
    let input = read_input();
    let ref_str = match input.get("ref").and_then(Value::as_str) {
        Some(r) => r,
        None => fail(exit_code::INVALID_INPUT, "missing 'ref' field"),
    };

    let key = ref_str.strip_prefix("vault://").unwrap_or(ref_str);
    let vault = active_vault();

    // Try structured: match by credential id
    if let Some(cred) = vault.credentials.iter().find(|c| c.id == key) {
        // Return the first data field value
        if let Some(val) = cred.data.values().next() {
            println!("{}", serde_json::json!({ "secret": val }));
            return;
        }
    }

    // Try structured: match by label
    if let Some(cred) = vault.credentials.iter().find(|c| c.label == key) {
        if let Some(val) = cred.data.values().next() {
            println!("{}", serde_json::json!({ "secret": val }));
            return;
        }
    }

    // Try legacy: flat key in old-style vault
    // (already handled by migration on load — keys become credential ids)
    fail(
        exit_code::GENERIC,
        format!(
            "secret not found for '{key}' (vault file: {})",
            vault_path().display()
        ),
    );
}

// ── CLI Dispatch ─────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flags: Vec<&str> = args.iter().map(|s| s.as_str()).skip(1).collect();

    if flags.contains(&"--describe") {
        print_manifest(&manifest());
        return;
    }
    if flags.contains(&"--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if flags.contains(&"--list") {
        cmd_list();
        return;
    }
    if let Some(pos) = flags.iter().position(|a| *a == "--get") {
        let id = flags
            .get(pos + 1)
            .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "--get requires <id> argument"));
        cmd_get(id);
        return;
    }
    if flags.contains(&"--create") {
        cmd_create();
        return;
    }
    if let Some(pos) = flags.iter().position(|a| *a == "--update") {
        let id = flags
            .get(pos + 1)
            .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "--update requires <id> argument"));
        cmd_update(id);
        return;
    }
    if let Some(pos) = flags.iter().position(|a| *a == "--delete") {
        let id = flags
            .get(pos + 1)
            .unwrap_or_else(|| fail(exit_code::INVALID_INPUT, "--delete requires <id> argument"));
        cmd_delete(id);
        return;
    }

    // Default: resolve mode
    cmd_resolve();
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn with_temp_vault<F>(f: F)
    where
        F: FnOnce(PathBuf),
    {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-vault.json");
        let prev_file = std::env::var("NGALIR_VAULT_FILE").ok();
        let prev_key = std::env::var("NGALIR_VAULT_KEY").ok();
        unsafe {
            std::env::set_var("NGALIR_VAULT_FILE", path.to_str().unwrap());
            std::env::remove_var("NGALIR_VAULT_KEY");
        }
        f(path);
        unsafe {
            match prev_file {
                Some(v) => std::env::set_var("NGALIR_VAULT_FILE", v),
                None => std::env::remove_var("NGALIR_VAULT_FILE"),
            }
            match prev_key {
                Some(v) => std::env::set_var("NGALIR_VAULT_KEY", v),
                None => std::env::remove_var("NGALIR_VAULT_KEY"),
            }
        }
        drop(dir);
    }

    fn write_vault_file(path: &PathBuf, content: &str) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, content).unwrap();
    }

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
    fn test_empty_vault_when_no_file() {
        with_temp_vault(|_path| {
            let vault = active_vault();
            assert!(vault.credentials.is_empty());
            assert_eq!(vault.version, 1);
        });
    }

    #[test]
    #[serial]
    fn test_create_and_list() {
        with_temp_vault(|_path| {
            let input = serde_json::json!({
                "credential_spec_id": "google_service_account",
                "label": "My Google SA",
                "auth_type": "custom",
                "data": {
                    "credentials": "{\"private_key\":\"test\"}"
                }
            });
            let cred = create_credential(&input);
            save_credential(&cred);
            assert!(cred.id.starts_with("cred_"));
            assert_eq!(cred.label, "My Google SA");

            let list = list_credentials();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0]["label"], "My Google SA");
        });
    }

    #[test]
    #[serial]
    fn test_create_get_update_delete() {
        with_temp_vault(|_path| {
            let create_input = serde_json::json!({
                "credential_spec_id": "slack_api",
                "label": "My Slack",
                "auth_type": "oauth2",
                "data": {"access_token": "xoxb-xxx"}
            });
            let cred = create_credential(&create_input);
            save_credential(&cred);
            let id = cred.id.clone();

            // Get
            let got = get_credential(&id).expect("should exist");
            assert_eq!(got.label, "My Slack");
            assert_eq!(
                got.data.get("access_token").and_then(Value::as_str),
                Some("xoxb-xxx")
            );

            // Update
            let update_input = serde_json::json!({"label": "My Slack Updated"});
            let updated = update_credential(&id, &update_input).expect("should update");
            assert_eq!(updated.label, "My Slack Updated");

            // Delete
            assert!(delete_credential(&id));
            assert!(list_credentials().is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_legacy_flat_vault_migration() {
        with_temp_vault(|path| {
            let content = r#"{"db/prod": "postgres://u:p@host/db", "api/key": "sk-xxx"}"#;
            write_vault_file(&path, content);

            let vault = active_vault();
            assert_eq!(vault.credentials.len(), 2);

            // Resolve should still work
            let result = resolve_ref("db/prod");
            assert_eq!(result.as_deref(), Some("postgres://u:p@host/db"));
        });
    }

    fn resolve_ref(key: &str) -> Option<String> {
        let vault = active_vault();
        if let Some(cred) = vault.credentials.iter().find(|c| c.id == key) {
            return cred
                .data
                .values()
                .next()
                .and_then(Value::as_str)
                .map(String::from);
        }
        if let Some(cred) = vault.credentials.iter().find(|c| c.label == key) {
            return cred
                .data
                .values()
                .next()
                .and_then(Value::as_str)
                .map(String::from);
        }
        None
    }

    #[test]
    #[serial]
    fn test_resolve_by_id() {
        with_temp_vault(|_path| {
            let input = serde_json::json!({
                "credential_spec_id": "slack_api",
                "label": "My Slack",
                "auth_type": "oauth2",
                "data": {"access_token": "xoxb-xxx"}
            });
            let cred = create_credential(&input);
            save_credential(&cred);

            let result = resolve_ref(&cred.id);
            assert_eq!(result.as_deref(), Some("xoxb-xxx"));
        });
    }

    #[test]
    #[serial]
    fn test_encryption_roundtrip() {
        with_temp_vault(|path| {
            let key_raw = b"01234567890123456789012345678901";
            let key_b64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key_raw);
            unsafe {
                std::env::set_var("NGALIR_VAULT_KEY", &key_b64);
            }

            let input = serde_json::json!({
                "credential_spec_id": "test",
                "label": "Encrypted Test",
                "auth_type": "api_key",
                "data": {"api_key": "sk-super-secret"}
            });
            let cred = create_credential(&input);
            save_credential(&cred);

            // File should be binary (encrypted)
            let raw = std::fs::read(&path).unwrap();
            assert!(!raw.is_empty());
            assert!(std::str::from_utf8(&raw).is_err());

            // List should still work (decrypts on read)
            let list = list_credentials();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0]["label"], "Encrypted Test");
        });
    }

    #[test]
    #[serial]
    fn test_get_nonexistent_returns_none() {
        with_temp_vault(|_path| {
            assert!(get_credential("cred_nonexistent").is_none());
        });
    }

    #[test]
    #[serial]
    fn test_delete_nonexistent_returns_false() {
        with_temp_vault(|_path| {
            assert!(!delete_credential("cred_ghost"));
        });
    }

    #[test]
    fn test_generate_id_format() {
        let id = generate_id();
        assert!(id.starts_with("cred_"));
        assert!(id.len() > 5);
    }

    #[test]
    #[serial]
    fn test_encryption_key_from_env() {
        unsafe {
            std::env::remove_var("NGALIR_VAULT_KEY");
        }
        assert!(encryption_key().is_none());

        let key_bytes = [0u8; 32];
        let key_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key_bytes);
        unsafe {
            std::env::set_var("NGALIR_VAULT_KEY", &key_b64);
        }
        let key = encryption_key();
        assert!(key.is_some());
        assert_eq!(key.unwrap(), key_bytes);
        unsafe {
            std::env::remove_var("NGALIR_VAULT_KEY");
        }
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0xABu8; 32];
        let plaintext = b"hello vault secret";
        let encrypted = encrypt(plaintext, &key);
        assert_ne!(encrypted, plaintext);
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = [0x01u8; 32];
        let key2 = [0x02u8; 32];
        let encrypted = encrypt(b"test data", &key1);
        assert!(decrypt(&encrypted, &key2).is_none());
    }

    #[test]
    fn test_chrono_now_format() {
        let s = chrono_now();
        // ISO 8601 format: 2026-07-22T12:34:56Z
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[10..11], "T");
        assert_eq!(&s[13..14], ":");
        assert_eq!(&s[16..17], ":");
    }
}
