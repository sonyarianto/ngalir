//! Ngalir database query node (PostgreSQL).

use na_contract::{exit_code, fail, print_manifest, read_input, Manifest};
use serde_json::{json, Map, Value};
use sqlx::{postgres::PgRow, Column, Row};

fn manifest() -> Manifest {
    Manifest {
        name: "na-db-postgres".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Execute SQL queries against PostgreSQL databases.".to_string(),
        inputs: serde_json::json!({
            "type": "object",
            "properties": {
                "connection": { "type": "string", "description": "PostgreSQL DSN or vault:// ref" },
                "query": { "type": "string", "description": "SQL query to execute" }
            },
            "required": ["connection", "query"]
        }),
        outputs: serde_json::json!({
            "type": "object",
            "properties": {
                "rows": { "type": "array" },
                "row_count": { "type": "integer" }
            }
        }),
        secrets: vec!["connection".into()],
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

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(run());
}

async fn run() {
    let input = read_input();
    let conn_str = na_contract::read_secret("connection")
        .or_else(|| input["connection"].as_str().map(String::from))
        .unwrap_or_default();
    let query = input["query"].as_str().unwrap_or("");

    if conn_str.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'connection'");
    }
    if query.is_empty() {
        fail(exit_code::INVALID_INPUT, "missing 'query'");
    }

    let pool = sqlx::PgPool::connect(&conn_str).await.unwrap_or_else(|e| {
        fail(exit_code::GENERIC, format!("DB connection failed: {e}"));
    });

    let trimmed = query.trim().to_uppercase();
    let is_select = trimmed.starts_with("SELECT") || trimmed.starts_with("WITH");

    if is_select {
        let rows: Vec<PgRow> = sqlx::query::<sqlx::Postgres>(query)
            .fetch_all(&pool)
            .await
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("query failed: {e}")));

        let mut result = Vec::new();
        for row in &rows {
            let mut obj = Map::new();
            for col in row.columns() {
                let name = col.name().to_string();
                obj.insert(name, value_at(row, col.name()));
            }
            result.push(Value::Object(obj));
        }

        let output = json!({ "rows": result, "row_count": result.len() });
        println!("{output}");
    } else {
        let outcome = sqlx::query::<sqlx::Postgres>(query)
            .execute(&pool)
            .await
            .unwrap_or_else(|e| fail(exit_code::GENERIC, format!("query failed: {e}")));

        let output = json!({ "rows": [], "row_count": outcome.rows_affected() });
        println!("{output}");
    }
}

fn value_at(row: &PgRow, col: &str) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(col) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(col) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i16>, _>(col) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(col) {
        return v
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f32>, _>(col) {
        return v
            .and_then(|n| serde_json::Number::from_f64(n as f64))
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(col) {
        return v.map(Value::Bool).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(col) {
        return v.unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(col) {
        return v.map(Value::String).unwrap_or(Value::Null);
    }
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_structure() {
        let m = manifest();
        assert_eq!(m.name, "na-db-postgres");
        let required = m.inputs.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("connection")));
        assert!(required.contains(&serde_json::json!("query")));
        assert_eq!(m.secrets, vec!["connection"]);
    }
}
