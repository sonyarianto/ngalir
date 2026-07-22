use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};
use std::path::Path;

pub fn cmd_init_node() -> Result<()> {
    println!("╔══════════════════════════════════════════╗");
    println!("║  Ngalir Node Scaffold Generator         ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    let name = prompt("Node name (e.g., 'slack' -> na-slack): ")?;
    validate_name(&name)?;
    let description = prompt("One-line description: ")?;

    let input_fields = collect_input_fields();
    let output_fields = collect_output_fields();
    let cred_specs = collect_credential_specs();
    let has_tokio = prompt_bool("Does this node need async runtime (tokio)? [y/N]: ")?;

    let crate_dir = Path::new("crates").join(format!("na-{}", name));
    if crate_dir.exists() {
        anyhow::bail!("directory {} already exists", crate_dir.display());
    }
    std::fs::create_dir_all(crate_dir.join("src")).context("failed to create crate directory")?;

    write_cargo_toml(&crate_dir, &name, &description, &input_fields, has_tokio)?;
    write_main_rs(
        &crate_dir,
        &name,
        &description,
        &input_fields,
        &output_fields,
        &cred_specs,
        has_tokio,
    )?;
    register_workspace_member(&name)?;

    println!();
    println!("  Created na-{name}/ crate at {}", crate_dir.display());
    println!();
    println!("  Next steps:");
    println!("    cd {}", crate_dir.display());
    println!("    Implement the node logic in src/main.rs");
    println!("    cargo build -p na-{name}");

    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("node name cannot be empty");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        anyhow::bail!("node name must be alphanumeric (hyphens allowed)");
    }
    Ok(())
}

fn prompt(msg: &str) -> Result<String> {
    print!("{msg}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn prompt_bool(msg: &str) -> Result<bool> {
    let val = prompt(msg)?.to_lowercase();
    Ok(val == "y" || val == "yes" || val == "true")
}

#[derive(Debug)]
struct InputField {
    name: String,
    field_type: String,
    required: bool,
}

fn collect_input_fields() -> Vec<InputField> {
    println!("\n-- Input fields (JSON Schema properties) --");
    println!("  Leave field name empty to finish.");
    let mut fields = Vec::new();
    loop {
        let name = match prompt("  Field name: ") {
            Ok(n) if n.is_empty() => break,
            Ok(n) => n,
            Err(_) => break,
        };
        let field_type = prompt(&format!("  Type of '{name}' [string]: "))
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "string".to_string());
        let required = prompt_bool(&format!("  Is '{name}' required? [y/N]: ")).unwrap_or(false);
        fields.push(InputField {
            name,
            field_type,
            required,
        });
    }
    fields
}

#[derive(Debug)]
struct OutputField {
    name: String,
    field_type: String,
}

fn collect_output_fields() -> Vec<OutputField> {
    println!("\n-- Output fields (JSON Schema properties) --");
    println!("  Leave field name empty to finish.");
    let mut fields = Vec::new();
    loop {
        let name = match prompt("  Field name: ") {
            Ok(n) if n.is_empty() => break,
            Ok(n) => n,
            Err(_) => break,
        };
        let field_type = prompt(&format!("  Type of '{name}' [string]: "))
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "string".to_string());
        fields.push(OutputField { name, field_type });
    }
    fields
}

#[derive(Debug)]
enum CredAuthType {
    ApiKey,
    BasicAuth,
    OAuth2,
    Custom,
}

#[derive(Debug)]
struct CredSpec {
    id: String,
    label: String,
    auth_type: CredAuthType,
    fields: Vec<(String, String, bool)>,
    oauth: Option<OAuthSpec>,
}

#[derive(Debug)]
struct OAuthSpec {
    authorize_url: String,
    token_url: String,
    scopes: Vec<String>,
    client_id_env: String,
}

fn collect_credential_specs() -> Vec<CredSpec> {
    println!("\n-- Credential specs --");
    let has_creds = prompt_bool("Does this node need credentials? [y/N]: ").unwrap_or(false);
    if !has_creds {
        return vec![];
    }

    let mut specs = Vec::new();
    loop {
        println!();
        let id = match prompt("  Credential spec ID (e.g., 'slack_api'): ") {
            Ok(s) if s.is_empty() && specs.is_empty() => {
                println!("  (at least one credential spec needed)");
                continue;
            }
            Ok(s) if s.is_empty() => break,
            Ok(s) => s,
            Err(_) => break,
        };
        let label = prompt(&format!("  Label for '{id}' [{id}]: "))
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| id.clone());

        let auth_type = prompt_auth_type();

        let mut fields = Vec::new();
        println!("  Credential fields (leave key empty to finish):");
        loop {
            let key = match prompt("    Field key (e.g., 'api_key'): ") {
                Ok(k) if k.is_empty() => break,
                Ok(k) => k,
                Err(_) => break,
            };
            let flabel = prompt(&format!("    Label for '{key}' [{key}]: "))
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| key.clone());
            let required = prompt_bool("    Required? [Y/n]: ").unwrap_or(true);
            fields.push((key, flabel, required));
        }

        let mut oauth = None;
        if matches!(auth_type, CredAuthType::OAuth2) {
            println!("  OAuth2 configuration:");
            let authorize_url = prompt("    Authorize URL: ").unwrap_or_default();
            let token_url = prompt("    Token URL: ").unwrap_or_default();
            let scopes_str = prompt("    Scopes (comma-separated) []: ").unwrap_or_default();
            let scopes: Vec<String> = scopes_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let default_env = format!("NGALIR_{}_CLIENT_ID", id.to_uppercase());
            let client_id_env = prompt(&format!("    Client ID env var name [{default_env}]: "))
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or(default_env);
            if !authorize_url.is_empty() && !token_url.is_empty() {
                oauth = Some(OAuthSpec {
                    authorize_url,
                    token_url,
                    scopes,
                    client_id_env,
                });
            }
        }

        specs.push(CredSpec {
            id,
            label,
            auth_type,
            fields,
            oauth,
        });

        let another = prompt_bool("  Add another credential spec? [y/N]: ").unwrap_or(false);
        if !another {
            break;
        }
    }
    specs
}

fn prompt_auth_type() -> CredAuthType {
    println!("    Auth type:");
    println!("      1. API Key");
    println!("      2. Basic Auth");
    println!("      3. OAuth2");
    println!("      4. Custom");
    let choice = prompt("    Choice [1]: ").unwrap_or_default();
    match choice.trim() {
        "2" => CredAuthType::BasicAuth,
        "3" => CredAuthType::OAuth2,
        "4" => CredAuthType::Custom,
        _ => CredAuthType::ApiKey,
    }
}

fn write_cargo_toml(
    crate_dir: &Path,
    name: &str,
    description: &str,
    _input_fields: &[InputField],
    tokio: bool,
) -> Result<()> {
    let mut content = format!(
        r#"[package]
name = "na-{name}"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "{description}"

[dependencies]
na-contract = {{ path = "../na-contract" }}
serde_json = "1"
"#
    );
    if tokio {
        content.push_str(
            r#"tokio = { version = "1", features = ["full"] }
"#,
        );
    }
    std::fs::write(crate_dir.join("Cargo.toml"), content)?;
    Ok(())
}

fn write_main_rs(
    crate_dir: &Path,
    name: &str,
    description: &str,
    input_fields: &[InputField],
    output_fields: &[OutputField],
    cred_specs: &[CredSpec],
    has_tokio: bool,
) -> Result<()> {
    let node_name = format!("na-{}", name);

    let (imports, exec_body, test_connection_fn) = if has_tokio {
        let tokio_imports = if !cred_specs.is_empty() {
            "use na_contract::{print_manifest, read_input, Manifest, exit_code, fail, AuthType, CredentialField, CredentialSpec, OAuthConfig};\nuse serde_json::Value;"
        } else {
            "use na_contract::{print_manifest, read_input, Manifest, exit_code, fail};\nuse serde_json::Value;"
        };

        let has_creds = !cred_specs.is_empty();
        let test_fn = if has_creds {
            r#"
async fn test_connection(input: &Value) {
    // TODO: implement credential validation for this node type
    let out = serde_json::json!({"ok": true, "message": "connection test not implemented"});
    println!("{out}");
}
"#
        } else {
            ""
        };

        let exec = r#"    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(run());
}

async fn run() {
    // TODO: implement node logic
    let output = serde_json::json!({
        // "result": "TODO",
    });
    println!("{output}");
}"#;

        (tokio_imports, exec, test_fn)
    } else {
        let sync_imports = if !cred_specs.is_empty() {
            "use na_contract::{print_manifest, read_input, Manifest, exit_code, fail, AuthType, CredentialField, CredentialSpec, OAuthConfig};\nuse serde_json::Value;"
        } else {
            "use na_contract::{print_manifest, read_input, Manifest, exit_code, fail};\nuse serde_json::Value;"
        };
        let exec = r#"    // TODO: implement node logic
    let output = serde_json::json!({
        // "result": "TODO",
    });
    println!("{output}");
}

fn run() {"#;

        (sync_imports, exec, "")
    };

    let has_creds = !cred_specs.is_empty();
    let test_connection_block = if has_creds && has_tokio {
        r#"    if args.iter().any(|a| a == "--test-connection") {
        let input = read_input();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(test_connection(&input));
        return;
    }
"#
    } else if has_creds && !has_tokio {
        r#"    if args.iter().any(|a| a == "--test-connection") {
        let input = read_input();
        // TODO: implement synchronous test_connection
        let out = serde_json::json!({"ok": true, "message": "connection test not implemented"});
        println!("{out}");
        return;
    }
"#
    } else {
        ""
    };

    let input_props = render_input_properties(input_fields);
    let required_list = render_required_list(input_fields);
    let output_props = render_output_properties(output_fields);
    let cred_json = render_credential_specs_json(cred_specs);
    let use_cases = format!("\"{}\"", name);

    let content = format!(
        r#"//! {description}

{imports}

fn manifest() -> Manifest {{
    Manifest {{
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "{description}".to_string(),
        inputs: serde_json::json!({{
            "type": "object",
            "properties": {{
                {input_props}
            }}{required_list}
        }}),
        outputs: serde_json::json!({{
            "type": "object",
            "properties": {{
                {output_props}
            }}
        }}),
        secrets: vec![],
        credentials: {cred_json},
        streaming: false,
        idempotent: false,
        output_mode: None,
        use_cases: vec![{use_cases}],
        examples: vec![],
        see_also: vec![],
    }}
}}

fn main() {{
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--describe") {{
        print_manifest(&manifest());
        return;
    }}
    if args.iter().any(|a| a == "--version") {{
        println!("{{}}", env!("CARGO_PKG_VERSION"));
        return;
    }}
{test_connection_block}
    let input = read_input();
{exec_body}
{test_connection_fn}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn test_manifest_structure() {{
        let m = manifest();
        assert_eq!(m.name, "{node_name}");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
    }}

    #[test]
    fn test_describe_output() {{
        use std::process::Command;
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/{node_name}");
        let output = Command::new(&p)
            .arg("--describe")
            .output()
            .expect("spawn --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("{node_name}"));
    }}
}}
"#,
        description = description,
        imports = imports,
        input_props = input_props,
        required_list = required_list,
        output_props = output_props,
        cred_json = cred_json,
        use_cases = use_cases,
        test_connection_block = test_connection_block,
        exec_body = exec_body,
        test_connection_fn = test_connection_fn,
        node_name = node_name,
    );

    std::fs::write(crate_dir.join("src").join("main.rs"), content)?;
    Ok(())
}

fn render_input_properties(fields: &[InputField]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let props: Vec<String> = fields
        .iter()
        .map(|f| format!("\"{}\": {{ \"type\": \"{}\" }}", f.name, f.field_type))
        .collect();
    props.join(",\n                ")
}

fn render_required_list(fields: &[InputField]) -> String {
    let required: Vec<&str> = fields
        .iter()
        .filter(|f| f.required)
        .map(|f| f.name.as_str())
        .collect();
    if required.is_empty() {
        return String::new();
    }
    let items: Vec<String> = required.iter().map(|n| format!("\"{}\"", n)).collect();
    format!(",\n            \"required\": [{}]", items.join(", "))
}

fn render_output_properties(fields: &[OutputField]) -> String {
    if fields.is_empty() {
        return String::new();
    }
    let props: Vec<String> = fields
        .iter()
        .map(|f| format!("\"{}\": {{ \"type\": \"{}\" }}", f.name, f.field_type))
        .collect();
    props.join(",\n                ")
}

fn render_credential_specs_json(specs: &[CredSpec]) -> String {
    if specs.is_empty() {
        return "vec![]".to_string();
    }
    let mut items = Vec::new();
    for spec in specs {
        let auth_str = match spec.auth_type {
            CredAuthType::ApiKey => "AuthType::ApiKey",
            CredAuthType::BasicAuth => "AuthType::BasicAuth",
            CredAuthType::OAuth2 => "AuthType::OAuth2",
            CredAuthType::Custom => "AuthType::Custom",
        };
        let fields_json = if spec.fields.is_empty() {
            "vec![]".to_string()
        } else {
            let f_items: Vec<String> = spec
                .fields
                .iter()
                .map(|(k, l, r)| {
                    format!(
                        "CredentialField {{ key: \"{}\".into(), label: \"{}\".into(), input_type: \"text\".into(), required: {} }}",
                        k, l, r
                    )
                })
                .collect();
            format!("vec![{}]", f_items.join(", "))
        };
        let oauth_json = match &spec.oauth {
            Some(oauth) => {
                let scopes: Vec<String> = oauth
                    .scopes
                    .iter()
                    .map(|s| format!("\"{}\".into()", s))
                    .collect();
                let scopes_str = if scopes.is_empty() {
                    "vec![]".to_string()
                } else {
                    format!("vec![{}]", scopes.join(", "))
                };
                format!(
                    "Some(OAuthConfig {{ authorize_url: \"{}\".into(), token_url: \"{}\".into(), scopes: {}, client_id_env: \"{}\".into(), client_secret_env: None }})",
                    oauth.authorize_url, oauth.token_url, scopes_str, oauth.client_id_env
                )
            }
            None => "None".to_string(),
        };
        items.push(format!(
            "CredentialSpec {{ id: \"{}\".into(), label: \"{}\".into(), auth_type: {}, fields: {}, oauth: {} }}",
            spec.id, spec.label, auth_str, fields_json, oauth_json
        ));
    }
    format!("vec![{}]", items.join(", "))
}

fn register_workspace_member(name: &str) -> Result<()> {
    let workspace_path = Path::new("Cargo.toml");
    let content =
        std::fs::read_to_string(workspace_path).context("failed to read workspace Cargo.toml")?;

    let new_member = format!("    \"crates/na-{}\",", name);

    if content.contains(&new_member) {
        return Ok(());
    }

    let updated = content.replace("]", &format!("{}\n]", new_member));
    if updated == content {
        anyhow::bail!("failed to register workspace member");
    }
    std::fs::write(workspace_path, updated).context("failed to write workspace Cargo.toml")?;

    println!("  Registered crates/na-{} in workspace Cargo.toml", name);
    Ok(())
}
