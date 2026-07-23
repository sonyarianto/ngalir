use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;

use crate::flow;

pub(crate) fn history_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("NGALIR_HISTORY_FILE") {
        return PathBuf::from(p);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ngalir")
        .join("history.db")
}

pub(crate) struct HistoryDb {
    path: PathBuf,
}

impl HistoryDb {
    pub(crate) fn new(path: PathBuf) -> Result<Self> {
        let db = Self { path };
        db.init()?;
        Ok(db)
    }

    fn connect(&self) -> Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        Ok(conn)
    }

    fn init(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS flow_runs (
                flow_id       TEXT PRIMARY KEY,
                flow_name     TEXT NOT NULL,
                status        TEXT NOT NULL,
                started_at    TEXT NOT NULL,
                finished_at   TEXT,
                duration_ms   INTEGER,
                node_count    INTEGER NOT NULL DEFAULT 0,
                error         TEXT
            );
            CREATE TABLE IF NOT EXISTS node_runs (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                flow_id       TEXT NOT NULL,
                node_id       TEXT NOT NULL,
                node_type     TEXT NOT NULL,
                status        TEXT NOT NULL,
                started_at    TEXT,
                finished_at   TEXT,
                duration_ms   INTEGER,
                input         TEXT,
                output        TEXT,
                error         TEXT,
                UNIQUE(flow_id, node_id)
            );",
        )?;
        Ok(())
    }

    pub(crate) fn record_flow_start(
        &self,
        flow_id: &str,
        flow_name: &str,
        node_count: usize,
    ) -> Result<()> {
        let conn = self.connect()?;
        let now = flow::chrono_now();
        conn.execute(
            "INSERT OR REPLACE INTO flow_runs (flow_id, flow_name, status, started_at, node_count)
             VALUES (?1, ?2, 'running', ?3, ?4)",
            rusqlite::params![flow_id, flow_name, now, node_count],
        )?;
        Ok(())
    }

    pub(crate) fn record_flow_end(
        &self,
        flow_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.connect()?;
        let now = flow::chrono_now();
        let mut duration_ms: Option<i64> = None;
        if let Ok(start_row) = conn.query_row(
            "SELECT started_at FROM flow_runs WHERE flow_id = ?1",
            rusqlite::params![flow_id],
            |row| row.get::<_, String>(0),
        ) {
            if let (Ok(start), Ok(end)) = (
                flow::parse_iso8601_ms(&start_row),
                flow::parse_iso8601_ms(&now),
            ) {
                duration_ms = Some(end - start);
            }
        }
        conn.execute(
            "UPDATE flow_runs SET status = ?1, finished_at = ?2, duration_ms = ?3, error = ?4 WHERE flow_id = ?5",
            rusqlite::params![status, now, duration_ms, error, flow_id],
        )?;
        Ok(())
    }

    pub(crate) fn record_node_start(
        &self,
        flow_id: &str,
        node_id: &str,
        node_type: &str,
        input: Option<&Value>,
    ) -> Result<()> {
        let conn = self.connect()?;
        let now = flow::chrono_now();
        let input_str = input.map(|v| v.to_string());
        conn.execute(
            "INSERT OR REPLACE INTO node_runs (flow_id, node_id, node_type, status, started_at, input)
             VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
            rusqlite::params![flow_id, node_id, node_type, now, input_str],
        )?;
        Ok(())
    }

    pub(crate) fn record_node_end(
        &self,
        flow_id: &str,
        node_id: &str,
        _node_type: &str,
        status: &str,
        output: Option<&Value>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.connect()?;
        let now = flow::chrono_now();
        let output_str = output.map(|v| v.to_string());
        let mut duration_ms: Option<i64> = None;
        if let Ok(Some(start_str)) = conn.query_row(
            "SELECT started_at FROM node_runs WHERE flow_id = ?1 AND node_id = ?2",
            rusqlite::params![flow_id, node_id],
            |row| row.get::<_, Option<String>>(0),
        ) {
            if let (Ok(start), Ok(end)) = (
                flow::parse_iso8601_ms(&start_str),
                flow::parse_iso8601_ms(&now),
            ) {
                duration_ms = Some(end - start);
            }
        }
        conn.execute(
            "UPDATE node_runs SET status = ?1, finished_at = ?2, duration_ms = ?3, output = ?4, error = ?5
             WHERE flow_id = ?6 AND node_id = ?7",
            rusqlite::params![status, now, duration_ms, output_str, error, flow_id, node_id],
        )?;
        Ok(())
    }

    pub(crate) fn record_node_skipped(
        &self,
        flow_id: &str,
        node_id: &str,
        node_type: &str,
    ) -> Result<()> {
        let now = flow::chrono_now();
        let conn = self.connect()?;
        conn.execute(
            "INSERT OR REPLACE INTO node_runs (flow_id, node_id, node_type, status, started_at, finished_at)
             VALUES (?1, ?2, ?3, 'skipped', ?4, ?5)",
            rusqlite::params![flow_id, node_id, node_type, now, now],
        )?;
        Ok(())
    }

    pub(crate) fn list_runs(&self) -> Result<Vec<Value>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT flow_id, flow_name, status, started_at, finished_at, duration_ms, node_count, error
             FROM flow_runs ORDER BY started_at DESC LIMIT 100",
        )?;
        let rows = stmt.query_map([], |row| {
            let flow_id: String = row.get(0)?;
            let flow_name: String = row.get(1)?;
            let status: String = row.get(2)?;
            let started_at: String = row.get(3)?;
            let finished_at: Option<String> = row.get(4)?;
            let duration_ms: Option<i64> = row.get(5)?;
            let node_count: i64 = row.get(6)?;
            let error: Option<String> = row.get(7)?;
            Ok(serde_json::json!({
                "flow_id": flow_id,
                "flow_name": flow_name,
                "status": status,
                "started_at": started_at,
                "finished_at": finished_at,
                "duration_ms": duration_ms,
                "node_count": node_count,
                "error": error,
            }))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub(crate) fn get_run(&self, flow_id: &str) -> Result<Option<Value>> {
        let conn = self.connect()?;
        let flow = conn
            .query_row(
                "SELECT flow_id, flow_name, status, started_at, finished_at, duration_ms, node_count, error
                 FROM flow_runs WHERE flow_id = ?1",
                rusqlite::params![flow_id],
                |row| {
                    let flow_id: String = row.get(0)?;
                    let flow_name: String = row.get(1)?;
                    let status: String = row.get(2)?;
                    let started_at: String = row.get(3)?;
                    let finished_at: Option<String> = row.get(4)?;
                    let duration_ms: Option<i64> = row.get(5)?;
                    let node_count: i64 = row.get(6)?;
                    let error: Option<String> = row.get(7)?;
                    Ok(serde_json::json!({
                        "flow_id": flow_id,
                        "flow_name": flow_name,
                        "status": status,
                        "started_at": started_at,
                        "finished_at": finished_at,
                        "duration_ms": duration_ms,
                        "node_count": node_count,
                        "error": error,
                    }))
                },
            )
            .ok();

        let Some(flow) = flow else { return Ok(None) };

        let mut stmt = conn.prepare(
            "SELECT node_id, node_type, status, started_at, finished_at, duration_ms, input, output, error
             FROM node_runs WHERE flow_id = ?1 ORDER BY id ASC",
        )?;
        let node_rows = stmt.query_map(rusqlite::params![flow_id], |row| {
            let node_id: String = row.get(0)?;
            let node_type: String = row.get(1)?;
            let status: String = row.get(2)?;
            let started_at: Option<String> = row.get(3)?;
            let finished_at: Option<String> = row.get(4)?;
            let duration_ms: Option<i64> = row.get(5)?;
            let input: Option<String> = row.get(6)?;
            let output: Option<String> = row.get(7)?;
            let error: Option<String> = row.get(8)?;
            Ok(serde_json::json!({
                "node_id": node_id,
                "node_type": node_type,
                "status": status,
                "started_at": started_at,
                "finished_at": finished_at,
                "duration_ms": duration_ms,
                "input": input.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "output": output.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                "error": error,
            }))
        })?;
        let mut nodes = Vec::new();
        for row in node_rows {
            nodes.push(row?);
        }

        Ok(Some(serde_json::json!({
            "flow": flow,
            "nodes": nodes,
        })))
    }
}
