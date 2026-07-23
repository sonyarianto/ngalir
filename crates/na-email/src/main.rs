use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::Value;
use std::time::Duration;

fn manifest() -> Manifest {
    Manifest {
        name: "na-email".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Sends an email via SMTP.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "to": { "type": "string", "description": "Recipient email address" },
                "subject": { "type": "string", "description": "Email subject" },
                "body": { "type": "string", "description": "Email body (plain text)" },
                "smtp_host": { "type": "string", "default": "localhost" },
                "smtp_port": { "type": "integer", "default": 25 },
                "username": { "type": "string", "description": "SMTP username (optional)" },
                "password": { "type": "string", "description": "SMTP password (optional)" }
            },
            "required": ["to", "subject", "body"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "sent": { "type": "boolean" },
                "message_id": { "type": "string" }
            }
        }),
        secrets: vec!["password".to_string()],
        credentials: vec![],
        streaming: false,
        idempotent: true,
        output_mode: None,
        use_cases: vec!["email".into(), "notify".into(), "smtp".into()],
        examples: vec![],
        see_also: vec![],
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
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

    let to = match input.get("to").and_then(Value::as_str) {
        Some(v) => v.to_string(),
        None => fail(exit_code::INVALID_INPUT, "missing string field `to`"),
    };
    let subject = match input.get("subject").and_then(Value::as_str) {
        Some(v) => v.to_string(),
        None => fail(exit_code::INVALID_INPUT, "missing string field `subject`"),
    };
    let body = match input.get("body").and_then(Value::as_str) {
        Some(v) => v.to_string(),
        None => fail(exit_code::INVALID_INPUT, "missing string field `body`"),
    };

    let smtp_host = input
        .get("smtp_host")
        .and_then(Value::as_str)
        .unwrap_or("localhost");
    let smtp_port = input.get("smtp_port").and_then(Value::as_u64).unwrap_or(25) as u16;

    let username = input.get("username").and_then(Value::as_str);
    let password = std::env::var("NGALIR_SECRET_PASSWORD").ok().or_else(|| {
        input
            .get("password")
            .and_then(Value::as_str)
            .map(String::from)
    });

    let result = send_email(
        &to,
        &subject,
        &body,
        smtp_host,
        smtp_port,
        username,
        password.as_deref(),
    )
    .await;

    match result {
        Ok(_msg_id) => {
            let output = serde_json::json!({ "sent": true, "message_id": _msg_id });
            println!("{output}");
        }
        Err(e) => {
            fail(exit_code::GENERIC, format!("failed to send email: {e}"));
        }
    }
}

async fn send_email(
    to: &str,
    subject: &str,
    body: &str,
    smtp_host: &str,
    smtp_port: u16,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<String, String> {
    use lettre::message::Mailbox;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let email = Message::builder()
        .from(
            "ngalir@localhost"
                .parse::<Mailbox>()
                .map_err(|e| e.to_string())?,
        )
        .to(to.parse::<Mailbox>().map_err(|e| e.to_string())?)
        .subject(subject.to_string())
        .body(body.to_string())
        .map_err(|e| e.to_string())?;

    let creds = username
        .zip(password)
        .map(|(u, p)| Credentials::new(u.to_string(), p.to_string()));

    let mailer = if let Some(creds) = creds {
        AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host)
            .map_err(|e| e.to_string())?
            .port(smtp_port)
            .credentials(creds)
            .timeout(Some(Duration::from_secs(30)))
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host)
            .port(smtp_port)
            .timeout(Some(Duration::from_secs(30)))
            .build()
    };

    let _result = mailer.send(email).await.map_err(|e| e.to_string())?;
    Ok("sent".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-email");
        assert!(!m.version.is_empty());
        assert!(!m.description.is_empty());
        assert!(!m.streaming);
        assert!(m.idempotent);
        assert!(m.secrets.contains(&"password".to_string()));
        assert!(m.inputs.get("required").is_some());
    }

    #[test]
    fn test_describe_output() {
        let bin = email_bin();
        let output = Command::new(&bin)
            .arg("--describe")
            .output()
            .expect("spawn na-email --describe");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("na-email"));
        assert!(stdout.contains("\"secrets\""));
        assert!(stdout.contains("\"password\""));
    }

    fn email_bin() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("../../target/debug/na-email");
        p
    }
}
