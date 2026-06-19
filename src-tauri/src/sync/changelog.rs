use crate::db::database::get_connection;
use nalu_shared::sync_protocol::{ChangelogEntry, SyncAck};

use super::is_sync_applying;

/// Record a data change into the local changelog table.
/// Accepts the connection directly to avoid re-locking the DB mutex.
pub fn record_change(
    conn: &rusqlite::Connection,
    table_name: &str,
    row_id: &str,
    operation: &str,
    payload: &str,
) -> Result<(), String> {
    if is_sync_applying() {
        return Ok(());
    }

    let ts = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "INSERT INTO changelog (table_name, row_id, operation, payload, client_ts, synced)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        rusqlite::params![table_name, row_id, operation, payload, ts],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Get all pending (unsynced) changelog entries.
pub fn get_pending_entries() -> Result<Vec<ChangelogEntry>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, table_name, row_id, operation, payload, client_ts, server_ts, synced
             FROM changelog WHERE synced = 0 ORDER BY id ASC",
        )
        .map_err(|e| e.to_string())?;
    let entries = stmt
        .query_map([], |row| {
            Ok(ChangelogEntry {
                id: Some(row.get(0)?),
                table_name: row.get(1)?,
                row_id: row.get(2)?,
                operation: row.get(3)?,
                payload: row.get(4)?,
                client_ts: row.get(5)?,
                server_ts: row.get(6)?,
                synced: row.get::<_, i32>(7)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(entries)
}

/// Mark specific entries as synced with their server_ts.
pub fn mark_synced(acks: &[SyncAck]) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    for ack in acks {
        conn.execute(
            "UPDATE changelog SET server_ts = ?1, synced = 1 WHERE id = ?2",
            rusqlite::params![ack.server_ts, ack.client_entry_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Get the latest server_ts we've seen (for pull requests).
pub fn get_last_server_ts() -> Result<i64, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let state_ts: Option<String> = conn
        .query_row(
            "SELECT value FROM sync_state WHERE key = 'last_server_ts'",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(ts) = state_ts.and_then(|value| value.parse::<i64>().ok()) {
        return Ok(ts);
    }

    let changelog_ts: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(server_ts), 0) FROM changelog WHERE synced = 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO sync_state (key, value) VALUES ('last_server_ts', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![changelog_ts.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(changelog_ts)
}

/// Persist the latest server changelog cursor this client has applied.
pub fn set_last_server_ts(server_ts: i64) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "INSERT INTO sync_state (key, value) VALUES ('last_server_ts', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![server_ts.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
