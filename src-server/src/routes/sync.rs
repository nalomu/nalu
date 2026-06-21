use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use nalu_shared::sync_protocol::{
    ChangelogEntry, OP_DELETE, OP_INSERT, OP_UPDATE, SYNC_TABLES, SyncAck, SyncPullRequest,
    SyncPullResponse, SyncPushRequest, SyncPushResponse,
};

use crate::state::{self, SharedState};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/sync/push", post(push))
        .route("/sync/pull", post(pull))
}

/// POST /api/sync/push
async fn push(
    State(state): State<SharedState>,
    Json(req): Json<SyncPushRequest>,
) -> Result<Json<SyncPushResponse>, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;
    let mut accepted = Vec::new();
    let mut conflicts = Vec::new();

    for entry in &req.entries {
        if !SYNC_TABLES.contains(&entry.table_name.as_str()) {
            continue;
        }

        // Check if server already has a newer version of this row
        let existing_ts: Option<i64> = conn
            .query_row(
                "SELECT MAX(server_ts) FROM server_changelog WHERE table_name = ?1 AND row_id = ?2",
                rusqlite::params![entry.table_name, entry.row_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        if let Some(_ts) = existing_ts {
            // Server has data — fetch the latest payload for this row
            let latest_payload: Option<(String, String, i64)> = conn
                .query_row(
                    "SELECT operation, payload, server_ts FROM server_changelog WHERE table_name = ?1 AND row_id = ?2 ORDER BY server_ts DESC LIMIT 1",
                    rusqlite::params![entry.table_name, entry.row_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .ok();

            if let Some((operation, payload, ts)) = latest_payload
                && ts > req.last_server_ts
            {
                // Server version is newer — conflict, send server version back
                conflicts.push(ChangelogEntry {
                    id: entry.id,
                    table_name: entry.table_name.clone(),
                    row_id: entry.row_id.clone(),
                    operation,
                    payload,
                    client_ts: ts,
                    server_ts: Some(ts),
                    synced: false,
                });
                continue; // Don't apply this client entry
            }
        }

        // Accept: insert into server_changelog and apply to the data table
        conn.execute(
            "INSERT INTO server_changelog (device_id, table_name, row_id, operation, payload, client_ts) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                req.device_id,
                entry.table_name,
                entry.row_id,
                entry.operation,
                entry.payload,
                entry.client_ts,
            ],
        )
        .map_err(|e| (status::ise(), e.to_string()))?;

        let server_ts: i64 = conn.last_insert_rowid();

        // Apply to the actual data table
        apply_entry(&conn, entry)?;

        accepted.push(SyncAck {
            client_entry_id: entry.id.unwrap_or(0),
            server_ts,
        });
    }

    Ok(Json(SyncPushResponse {
        accepted,
        conflicts,
    }))
}

/// POST /api/sync/pull
async fn pull(
    State(state): State<SharedState>,
    Json(req): Json<SyncPullRequest>,
) -> Result<Json<SyncPullResponse>, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;

    let mut stmt = conn
        .prepare(
            "SELECT server_ts, device_id, table_name, row_id, operation, payload, client_ts
             FROM server_changelog
             WHERE server_ts > ?1 AND device_id != ?2
             ORDER BY server_ts ASC",
        )
        .map_err(|e| (status::ise(), e.to_string()))?;

    let entries: Vec<ChangelogEntry> = stmt
        .query_map(
            rusqlite::params![req.last_server_ts, req.device_id],
            |row| {
                Ok(ChangelogEntry {
                    id: None,
                    table_name: row.get(2)?,
                    row_id: row.get(3)?,
                    operation: row.get(4)?,
                    payload: row.get(5)?,
                    client_ts: row.get(6)?,
                    server_ts: Some(row.get(0)?),
                    synced: false,
                })
            },
        )
        .map_err(|e| (status::ise(), e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let latest_server_ts: i64 = entries
        .last()
        .and_then(|e| e.server_ts)
        .unwrap_or(req.last_server_ts);

    Ok(Json(SyncPullResponse {
        entries,
        latest_server_ts,
    }))
}

fn apply_entry(
    conn: &rusqlite::Connection,
    entry: &ChangelogEntry,
) -> Result<(), (StatusCode, String)> {
    if !SYNC_TABLES.contains(&entry.table_name.as_str()) {
        return Ok(());
    }

    match entry.operation.as_str() {
        OP_INSERT | OP_UPDATE => {
            // Upsert: try update first, if no rows affected, insert
            upsert_row(conn, &entry.table_name, &entry.row_id, &entry.payload)?;
        }
        OP_DELETE => {
            let key_column = sync_key_column(&entry.table_name)?;
            conn.execute(
                &format!("DELETE FROM {} WHERE {} = ?1", entry.table_name, key_column),
                rusqlite::params![entry.row_id],
            )
            .map_err(|e| (status::ise(), e.to_string()))?;
        }
        _ => {}
    }
    Ok(())
}

fn upsert_row(
    conn: &rusqlite::Connection,
    table: &str,
    row_id: &str,
    payload: &str,
) -> Result<(), (StatusCode, String)> {
    let key_column = sync_key_column(table)?;

    // Parse JSON payload into a map of field → value
    let fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(payload).map_err(|e| (status::ise(), e.to_string()))?;

    if fields.is_empty() {
        return Ok(());
    }

    let mut set_clauses = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    for (key, value) in &fields {
        validate_identifier(key)?;
        if key == key_column {
            continue;
        }
        set_clauses.push(format!("{} = ?", key));
        values.push(json_value_to_sql(value));
    }

    if set_clauses.is_empty() {
        return Ok(());
    }

    // Try UPDATE first (table name is from our own changelog, safe to interpolate)
    let set_str = set_clauses.join(", ");
    let sql = format!("UPDATE {} SET {} WHERE {} = ?", table, set_str, key_column);

    // Build params: setters first, then row_id for WHERE clause
    let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = values;
    all_params.push(Box::new(row_id.to_string()) as Box<dyn rusqlite::types::ToSql>);
    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        all_params.iter().map(|v| v.as_ref()).collect();
    let affected = conn
        .execute(&sql, rusqlite::params_from_iter(params_refs.iter()))
        .map_err(|e| (status::ise(), e.to_string()))?;

    // If no row updated, INSERT
    if affected == 0 {
        let columns: Vec<String> = fields.keys().cloned().collect();
        for column in &columns {
            validate_identifier(column)?;
        }
        let placeholders: Vec<String> = columns.iter().map(|_| "?".to_string()).collect();

        let mut insert_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for key in &columns {
            let value = &fields[key];
            insert_values.push(json_value_to_sql(value));
        }

        let insert_sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let insert_params: Vec<&dyn rusqlite::types::ToSql> =
            insert_values.iter().map(|v| v.as_ref()).collect();
        conn.execute(
            &insert_sql,
            rusqlite::params_from_iter(insert_params.iter()),
        )
        .map_err(|e| (status::ise(), e.to_string()))?;
    }

    Ok(())
}

fn json_value_to_sql(value: &serde_json::Value) -> Box<dyn rusqlite::types::ToSql> {
    match value {
        serde_json::Value::Null => Box::new(rusqlite::types::Null),
        serde_json::Value::String(s) => Box::new(s.clone()),
        serde_json::Value::Number(n) if n.is_i64() => Box::new(n.as_i64().unwrap()),
        serde_json::Value::Number(n) if n.is_u64() => Box::new(n.as_u64().unwrap() as i64),
        serde_json::Value::Number(n) if n.is_f64() => Box::new(n.as_f64().unwrap()),
        serde_json::Value::Bool(b) => Box::new(*b as i32),
        _ => Box::new(value.to_string()),
    }
}

fn sync_key_column(table_name: &str) -> Result<&'static str, (StatusCode, String)> {
    if !SYNC_TABLES.contains(&table_name) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unsupported sync table: {}", table_name),
        ));
    }
    Ok(match table_name {
        "task_groups" => "project",
        _ => "id",
    })
}

fn validate_identifier(identifier: &str) -> Result<(), (StatusCode, String)> {
    let valid = !identifier.is_empty()
        && identifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if valid {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid sync payload field: {}", identifier),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE task_groups (
                project TEXT PRIMARY KEY,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE task_columns (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL DEFAULT 'default',
                name TEXT NOT NULL DEFAULT '任务',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL DEFAULT 'default',
                title TEXT NOT NULL,
                done INTEGER NOT NULL DEFAULT 0,
                progress INTEGER NOT NULL DEFAULT 0,
                column_id TEXT NOT NULL DEFAULT '',
                position INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                scheduled_start_at TEXT,
                scheduled_end_at TEXT,
                reminder_minutes INTEGER NOT NULL DEFAULT 0,
                completed_at TEXT,
                repeat_type TEXT NOT NULL DEFAULT 'none',
                recurrence_series_id TEXT,
                recurrence_sequence INTEGER,
                recurrence_origin_at TEXT,
                recurrence_detached INTEGER NOT NULL DEFAULT 0
            );
            ",
        )
        .unwrap();
        conn
    }

    #[test]
    fn upsert_row_uses_project_as_task_group_key() {
        let conn = test_conn();

        upsert_row(
            &conn,
            "task_groups",
            "work",
            r#"{"project":"work","sort_order":2}"#,
        )
        .unwrap();

        let sort_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM task_groups WHERE project = 'work'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sort_order, 2);

        upsert_row(
            &conn,
            "task_groups",
            "work",
            r#"{"project":"work","sort_order":5}"#,
        )
        .unwrap();

        let sort_order: i64 = conn
            .query_row(
                "SELECT sort_order FROM task_groups WHERE project = 'work'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sort_order, 5);
    }

    #[test]
    fn delete_entry_uses_project_as_task_group_key() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO task_groups (project, sort_order) VALUES ('work', 1)",
            [],
        )
        .unwrap();

        apply_entry(
            &conn,
            &ChangelogEntry {
                id: None,
                table_name: "task_groups".to_string(),
                row_id: "work".to_string(),
                operation: OP_DELETE.to_string(),
                payload: "{}".to_string(),
                client_ts: 1,
                server_ts: None,
                synced: false,
            },
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_groups WHERE project = 'work'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn upsert_row_rejects_unknown_tables_and_invalid_fields() {
        let conn = test_conn();

        assert!(upsert_row(&conn, "unknown", "1", r#"{"id":"1"}"#).is_err());
        assert!(
            upsert_row(
                &conn,
                "task_columns",
                "1",
                r#"{"id":"1","name = 'x' --":"bad"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn upsert_row_accepts_scheduled_task_payload_and_preserves_nulls() {
        let conn = test_conn();

        upsert_row(
            &conn,
            "tasks",
            "task-1",
            r#"{"id":"task-1","project":"2026-06-21","title":"Plan","done":false,"progress":0,"column_id":"","position":0,"created_at":"2026-06-21T08:00:00Z","updated_at":"2026-06-21T08:00:00Z","scheduled_start_at":"2026-06-21T09:00:00","scheduled_end_at":"2026-06-21T10:00:00","reminder_minutes":10,"completed_at":null,"repeat_type":"none","recurrence_series_id":null,"recurrence_sequence":null,"recurrence_origin_at":null,"recurrence_detached":false}"#,
        )
        .unwrap();

        let row: (String, Option<String>, i64, Option<String>, i64) = conn
            .query_row(
                "SELECT scheduled_start_at, completed_at, reminder_minutes, recurrence_series_id, recurrence_detached FROM tasks WHERE id = 'task-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();

        assert_eq!(row.0, "2026-06-21T09:00:00");
        assert_eq!(row.1, None);
        assert_eq!(row.2, 10);
        assert_eq!(row.3, None);
        assert_eq!(row.4, 0);
    }
}

mod status {
    pub fn ise() -> axum::http::StatusCode {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    }
}
