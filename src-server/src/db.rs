use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub type DbConn = Arc<Mutex<Connection>>;

pub fn open(path: &Path) -> Result<DbConn, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .map_err(|e| e.to_string())?;
    init_schema(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tasks (
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

        CREATE TABLE IF NOT EXISTS task_columns (
            id TEXT PRIMARY KEY,
            project TEXT NOT NULL DEFAULT 'default',
            name TEXT NOT NULL DEFAULT '任务',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS task_groups (
            project TEXT PRIMARY KEY,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS notes (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            note_type TEXT NOT NULL DEFAULT 'memo',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS schedules (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            scheduled_at TEXT NOT NULL,
            reminder_minutes INTEGER NOT NULL DEFAULT 5,
            done INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS server_changelog (
            server_ts INTEGER PRIMARY KEY AUTOINCREMENT,
            device_id TEXT NOT NULL,
            table_name TEXT NOT NULL,
            row_id TEXT NOT NULL,
            operation TEXT NOT NULL,
            payload TEXT NOT NULL,
            client_ts INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_server_changelog_ts
            ON server_changelog(server_ts);

        CREATE INDEX IF NOT EXISTS idx_server_changelog_table_row
            ON server_changelog(table_name, row_id);

        CREATE TABLE IF NOT EXISTS devices (
            device_id TEXT PRIMARY KEY,
            device_name TEXT NOT NULL,
            paired_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )
    .map_err(|e| e.to_string())?;

    for (column, definition) in [
        ("scheduled_start_at", "TEXT"),
        ("scheduled_end_at", "TEXT"),
        ("reminder_minutes", "INTEGER NOT NULL DEFAULT 0"),
        ("completed_at", "TEXT"),
        ("repeat_type", "TEXT NOT NULL DEFAULT 'none'"),
        ("recurrence_series_id", "TEXT"),
        ("recurrence_sequence", "INTEGER"),
        ("recurrence_origin_at", "TEXT"),
        ("recurrence_detached", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        ensure_column(conn, "tasks", column, definition)?;
    }

    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_scheduled_start_at
            ON tasks (scheduled_start_at);

        CREATE INDEX IF NOT EXISTS idx_tasks_recurrence_series_id
            ON tasks (recurrence_series_id);
        ",
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| e.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition),
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
