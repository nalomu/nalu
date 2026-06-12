use rusqlite::Connection;
use std::sync::Mutex;

/// Default column names for new groups
pub const DEFAULT_COLUMNS: &[&str] = &["重要", "一般"];

pub static DB: std::sync::LazyLock<Mutex<Option<Connection>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

pub fn init(path: &std::path::Path) -> Result<(), String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            project TEXT NOT NULL DEFAULT 'default',
            title TEXT NOT NULL,
            done INTEGER NOT NULL DEFAULT 0,
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

        CREATE TABLE IF NOT EXISTS clipboard_history (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            content_type TEXT NOT NULL DEFAULT 'text',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS schedules (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            scheduled_at TEXT NOT NULL,
            reminder_minutes INTEGER NOT NULL DEFAULT 5,
            done INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS alarms (
            id TEXT PRIMARY KEY,
            time TEXT NOT NULL,
            label TEXT NOT NULL DEFAULT '',
            repeat TEXT NOT NULL DEFAULT 'none',
            active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS mysql_users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL DEFAULT '',
            databases TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )
    .map_err(|e| e.to_string())?;

    // Run kanban board migration
    migrate_kanban_schema(&conn)?;

    let mut db = DB.lock().map_err(|e| e.to_string())?;
    *db = Some(conn);
    Ok(())
}

/// Idempotent migration for kanban board support.
/// Adds progress/column_id/position to tasks and creates task_columns table.
fn migrate_kanban_schema(conn: &Connection) -> Result<(), String> {
    // Create task_columns table if not exists
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS task_columns (
            id TEXT PRIMARY KEY,
            project TEXT NOT NULL DEFAULT 'default',
            name TEXT NOT NULL DEFAULT '任务',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| e.to_string())?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS task_groups (
            project TEXT PRIMARY KEY,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| e.to_string())?;

    // Check if progress column already exists
    let has_progress: bool = conn.prepare("SELECT progress FROM tasks LIMIT 1").is_ok();

    if !has_progress {
        // Add new columns to tasks table
        conn.execute_batch(
            "ALTER TABLE tasks ADD COLUMN progress INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE tasks ADD COLUMN column_id TEXT NOT NULL DEFAULT '';
             ALTER TABLE tasks ADD COLUMN position INTEGER NOT NULL DEFAULT 0;",
        )
        .map_err(|e| e.to_string())?;

        // Sync progress with existing done field
        conn.execute_batch(
            "UPDATE tasks SET progress = 100 WHERE done = 1;
             UPDATE tasks SET progress = 0 WHERE done = 0;",
        )
        .map_err(|e| e.to_string())?;
    }

    // Migrate existing tasks into default columns per project
    migrate_existing_tasks_to_columns(conn)?;

    // Ensure default group always has default columns
    let default_col_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_columns WHERE project = 'default'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if default_col_count == 0 {
        for (idx, name) in DEFAULT_COLUMNS.iter().enumerate() {
            let col_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, 'default', ?2, ?3)",
                rusqlite::params![col_id, name, idx as i64],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    sync_task_groups(conn)?;

    Ok(())
}

fn sync_task_groups(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT project FROM (
                SELECT project FROM task_columns GROUP BY project
                UNION
                SELECT project FROM tasks GROUP BY project
             ) ORDER BY project ASC",
        )
        .map_err(|e| e.to_string())?;
    let projects: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    for project in projects {
        let next_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM task_groups",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "INSERT OR IGNORE INTO task_groups (project, sort_order) VALUES (?1, ?2)",
            rusqlite::params![project, next_order],
        )
        .map_err(|e| e.to_string())?;
    }

    let has_default_group: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_groups WHERE project = 'default'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if has_default_group == 0 {
        conn.execute(
            "INSERT INTO task_groups (project, sort_order) VALUES ('default', 0)",
            [],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Migrate existing tasks that have no column_id into per-project default columns.
fn migrate_existing_tasks_to_columns(conn: &Connection) -> Result<(), String> {
    // Find all tasks with empty column_id (unmigrated)
    let unmigrated_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE column_id = ''",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if unmigrated_count == 0 {
        return Ok(());
    }

    // Get distinct projects from unmigrated tasks
    let mut stmt = conn
        .prepare("SELECT DISTINCT project FROM tasks WHERE column_id = ''")
        .map_err(|e| e.to_string())?;
    let projects: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    for project in &projects {
        // Find or create default column for this project
        let existing_col: Option<String> = conn
            .query_row(
                "SELECT id FROM task_columns WHERE project = ?1 ORDER BY sort_order ASC LIMIT 1",
                rusqlite::params![project],
                |row| row.get(0),
            )
            .ok();

        let col_id = match existing_col {
            Some(id) => id,
            None => {
                // Create default columns (紧急, 重要, 一般) for new projects
                let mut first_id = String::new();
                for (idx, name) in DEFAULT_COLUMNS.iter().enumerate() {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    if idx == 0 {
                        first_id = new_id.clone();
                    }
                    conn.execute(
                        "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
                        rusqlite::params![new_id, project, name, idx as i64],
                    )
                    .map_err(|e| e.to_string())?;
                }
                first_id
            }
        };

        // Assign tasks to this column with position based on created_at DESC
        let mut task_stmt = conn
            .prepare(
                "SELECT id FROM tasks WHERE project = ?1 AND column_id = '' ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let task_ids: Vec<String> = task_stmt
            .query_map(rusqlite::params![project], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        for (pos, task_id) in task_ids.iter().enumerate() {
            conn.execute(
                "UPDATE tasks SET column_id = ?1, position = ?2 WHERE id = ?3",
                rusqlite::params![col_id, pos as i64, task_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

pub fn get_connection() -> Result<std::sync::MutexGuard<'static, Option<Connection>>, String> {
    let db = DB.lock().map_err(|e| e.to_string())?;
    if db.is_none() {
        return Err("Database not initialized".to_string());
    }
    Ok(db)
}
