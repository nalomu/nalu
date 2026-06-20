use crate::db::database::get_connection;
use crate::sync::changelog;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime};
use nalu_shared::sync_protocol::OP_INSERT;
use serde::{Deserialize, Serialize};

const TASK_SELECT: &str = "id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at, scheduled_start_at, scheduled_end_at, COALESCE(reminder_minutes,0), completed_at, COALESCE(repeat_type,'none'), recurrence_series_id, recurrence_sequence, recurrence_origin_at, COALESCE(recurrence_detached,0)";
const REPEAT_TYPES: &[&str] = &["none", "daily", "weekly", "monthly", "yearly"];
const RECURRENCE_BATCH_LIMIT: i64 = 100;
const RECURRENCE_WINDOW_DAYS: i64 = 365;
const RECURRENCE_REFILL_THRESHOLD_DAYS: i64 = 30;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub project: String,
    pub title: String,
    pub done: bool,
    pub progress: i32,
    pub column_id: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
    pub scheduled_start_at: Option<String>,
    pub scheduled_end_at: Option<String>,
    pub reminder_minutes: i32,
    pub completed_at: Option<String>,
    pub repeat_type: String,
    pub recurrence_series_id: Option<String>,
    pub recurrence_sequence: Option<i64>,
    pub recurrence_origin_at: Option<String>,
    pub recurrence_detached: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CalendarTaskInput {
    pub title: String,
    pub project: Option<String>,
    pub column_id: Option<String>,
    pub scheduled_start_at: String,
    pub scheduled_end_at: String,
    pub reminder_minutes: Option<i32>,
    pub repeat_type: Option<String>,
    pub done: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskColumn {
    pub id: String,
    pub project: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnWithTasks {
    pub column: TaskColumn,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupData {
    pub project: String,
    pub sort_order: i64,
    pub columns: Vec<ColumnWithTasks>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub task: Task,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnSnapshot {
    pub column: TaskColumn,
}

fn create_default_columns_for_project(
    conn: &rusqlite::Connection,
    project: &str,
) -> Result<Vec<TaskColumn>, String> {
    let mut columns = Vec::new();

    for (idx, name) in crate::db::database::DEFAULT_COLUMNS.iter().enumerate() {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, project, name, idx as i64],
        )
        .map_err(|e| e.to_string())?;

        columns.push(TaskColumn {
            id,
            project: project.to_string(),
            name: name.to_string(),
            sort_order: idx as i64,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    Ok(columns)
}

fn today_project() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn ensure_group_and_columns(conn: &rusqlite::Connection, project: &str) -> Result<String, String> {
    let existing_column: Option<String> = conn
        .query_row(
            "SELECT id FROM task_columns WHERE project = ?1 ORDER BY sort_order ASC LIMIT 1",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .ok();

    if let Some(column_id) = existing_column {
        let next_order = next_group_sort_order(conn).unwrap_or(0);
        conn.execute(
            "INSERT OR IGNORE INTO task_groups (project, sort_order) VALUES (?1, ?2)",
            rusqlite::params![project, next_order],
        )
        .map_err(|e| e.to_string())?;
        return Ok(column_id);
    }

    let next_order = next_group_sort_order(conn).unwrap_or(0);
    conn.execute(
        "INSERT OR IGNORE INTO task_groups (project, sort_order) VALUES (?1, ?2)",
        rusqlite::params![project, next_order],
    )
    .map_err(|e| e.to_string())?;

    let columns = create_default_columns_for_project(conn, project)?;
    columns
        .first()
        .map(|column| column.id.clone())
        .ok_or_else(|| "Failed to create default columns".to_string())
}

fn column_belongs_to_project(
    conn: &rusqlite::Connection,
    column_id: &str,
    project: &str,
) -> Result<bool, String> {
    let column_project: Option<String> = conn
        .query_row(
            "SELECT project FROM task_columns WHERE id = ?1",
            rusqlite::params![column_id],
            |row| row.get(0),
        )
        .ok();
    Ok(column_project.as_deref() == Some(project))
}

fn normalize_repeat_type(value: Option<String>) -> Result<String, String> {
    let repeat_type = value.unwrap_or_else(|| "none".to_string());
    if REPEAT_TYPES.contains(&repeat_type.as_str()) {
        Ok(repeat_type)
    } else {
        Err("INVALID_REPEAT_TYPE".to_string())
    }
}

fn project_from_start(value: &str) -> String {
    match value.split('T').next().filter(|date| !date.is_empty()) {
        Some(date) => date.to_string(),
        None => today_project(),
    }
}

fn is_date_project(project: &str) -> bool {
    let bytes = project.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn load_task(conn: &rusqlite::Connection, id: &str) -> Result<Task, String> {
    conn.query_row(
        &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
        rusqlite::params![id],
        task_from_row,
    )
    .map_err(|e| e.to_string())
}

// ---------- Legacy commands (kept for dashboard/AI compatibility) ----------

#[tauri::command]
pub fn get_tasks(project: Option<String>) -> Result<Vec<Task>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let tasks = match &project {
        Some(p) => {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {TASK_SELECT} FROM tasks WHERE project = ?1 ORDER BY created_at DESC"
                ))
                .map_err(|e| e.to_string())?;
            let rows: Vec<Task> = stmt
                .query_map(rusqlite::params![p], task_from_row)
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            rows
        }
        None => {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {TASK_SELECT} FROM tasks ORDER BY created_at DESC"
                ))
                .map_err(|e| e.to_string())?;
            let rows: Vec<Task> = stmt
                .query_map([], task_from_row)
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            rows
        }
    };

    Ok(tasks)
}

#[tauri::command]
pub fn add_task(title: String, project: Option<String>) -> Result<Task, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let project = project.unwrap_or_else(today_project);

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let column_id = ensure_group_and_columns(conn, &project)?;

    let max_pos = next_task_position(conn, &column_id).unwrap_or(0);

    conn.execute(
        "INSERT INTO tasks (id, project, title, progress, column_id, position) VALUES (?1, ?2, ?3, 0, ?4, ?5)",
        rusqlite::params![id, project, title, column_id, max_pos],
    )
    .map_err(|e| e.to_string())?;

    load_task(conn, &id)
}

#[tauri::command]
pub fn toggle_task(id: String) -> Result<bool, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let (was_done, column_id): (bool, String) = conn
        .query_row(
            "SELECT done, COALESCE(column_id,'') FROM tasks WHERE id = ?1",
            rusqlite::params![&id],
            |row| Ok((row.get::<_, i32>(0)? != 0, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    if was_done {
        conn.execute(
            "UPDATE tasks SET done = 0, progress = 0, completed_at = NULL, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![&id],
        )
        .map_err(|e| e.to_string())?;

        return Ok(false);
    }

    let next_position: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM tasks WHERE column_id = ?1",
            rusqlite::params![column_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE tasks SET done = 1, progress = 100, completed_at = COALESCE(completed_at, datetime('now')), position = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![next_position, &id],
    )
    .map_err(|e| e.to_string())?;

    Ok(true)
}

#[tauri::command]
pub fn complete_task_group(project: String) -> Result<usize, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let updated = conn
        .execute(
            "UPDATE tasks SET done = 1, progress = 100, completed_at = COALESCE(completed_at, datetime('now')), updated_at = datetime('now') WHERE project = ?1 AND done = 0",
            rusqlite::params![project],
        )
        .map_err(|e| e.to_string())?;

    Ok(updated)
}

#[tauri::command]
pub fn update_task(id: String, title: String) -> Result<Task, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE tasks SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![title, id],
    )
    .map_err(|e| e.to_string())?;

    conn.query_row(
        &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
        rusqlite::params![id],
        task_from_row,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_task(id: String) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Kanban board commands ----------

/// Get the full board data: all groups with their columns and tasks.
#[tauri::command]
pub fn get_board(include_future_recurring: Option<bool>) -> Result<Vec<GroupData>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    repair_board_task_placement(conn)?;
    sync_groups_from_columns(conn)?;

    let mut group_stmt = conn
        .prepare("SELECT project, sort_order FROM task_groups ORDER BY sort_order ASC, project ASC")
        .map_err(|e| e.to_string())?;
    let group_rows: Vec<(String, i64)> = group_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Get all columns grouped by project
    let mut col_stmt = conn
        .prepare("SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns ORDER BY project, sort_order ASC")
        .map_err(|e| e.to_string())?;
    let columns: Vec<TaskColumn> = col_stmt
        .query_map([], |row| {
            Ok(TaskColumn {
                id: row.get(0)?,
                project: row.get(1)?,
                name: row.get(2)?,
                sort_order: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    // Get all tasks
    let mut task_stmt = conn
        .prepare(&format!(
            "SELECT {TASK_SELECT} FROM tasks ORDER BY done ASC, position ASC, created_at DESC"
        ))
        .map_err(|e| e.to_string())?;
    let mut all_tasks: Vec<Task> = task_stmt
        .query_map([], task_from_row)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    if include_future_recurring != Some(true) {
        let today = today_project();
        all_tasks.retain(|task| {
            if task.done || task.recurrence_series_id.is_none() {
                return true;
            }
            match &task.scheduled_start_at {
                Some(start_at) => project_from_start(start_at) <= today,
                None => true,
            }
        });
    }

    // Build column data by group
    let mut columns_by_group: std::collections::HashMap<String, Vec<ColumnWithTasks>> =
        std::collections::HashMap::new();

    for col in columns {
        let col_tasks: Vec<Task> = all_tasks
            .iter()
            .filter(|t| t.column_id == col.id)
            .cloned()
            .collect();
        columns_by_group
            .entry(col.project.clone())
            .or_default()
            .push(ColumnWithTasks {
                column: col,
                tasks: col_tasks,
            });
    }

    let hide_future_empty_date_groups = include_future_recurring != Some(true);
    let today = today_project();

    Ok(group_rows
        .into_iter()
        .map(|(project, sort_order)| GroupData {
            columns: columns_by_group.remove(&project).unwrap_or_default(),
            project,
            sort_order,
        })
        .filter(|group| {
            if !hide_future_empty_date_groups
                || !is_date_project(&group.project)
                || group.project <= today
            {
                return true;
            }
            group.columns.iter().any(|column| !column.tasks.is_empty())
        })
        .collect())
}

/// Create an empty task group with the default columns.
#[tauri::command]
pub fn create_task_group(project: String) -> Result<GroupData, String> {
    let project = project.trim().to_string();
    if project.is_empty() {
        return Err("Group name cannot be empty".to_string());
    }

    let mut db = get_connection()?;
    let conn = db.as_mut().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let existing_count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM task_columns WHERE project = ?1",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if existing_count > 0 {
        return Err("Group already exists".to_string());
    }

    let next_order = next_group_sort_order(&tx)?;
    tx.execute(
        "INSERT INTO task_groups (project, sort_order) VALUES (?1, ?2)",
        rusqlite::params![project, next_order],
    )
    .map_err(|e| e.to_string())?;

    let columns = create_default_columns_for_project(&tx, &project)?
        .into_iter()
        .map(|column| ColumnWithTasks {
            column,
            tasks: Vec::new(),
        })
        .collect();

    tx.commit().map_err(|e| e.to_string())?;
    Ok(GroupData {
        project,
        sort_order: next_order,
        columns,
    })
}

/// Delete a group only when it has no incomplete tasks.
#[tauri::command]
pub fn delete_task_group(project: String) -> Result<(), String> {
    let project = project.trim().to_string();
    if project == "default" {
        return Err("DEFAULT_GROUP".to_string());
    }

    let mut db = get_connection()?;
    let conn = db.as_mut().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    sync_groups_from_columns(&tx)?;

    let incomplete_count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE project = ?1 AND done = 0",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if incomplete_count > 0 {
        return Err("HAS_INCOMPLETE_TASKS".to_string());
    }

    tx.execute(
        "DELETE FROM tasks WHERE project = ?1",
        rusqlite::params![project],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM task_columns WHERE project = ?1",
        rusqlite::params![project],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM task_groups WHERE project = ?1",
        rusqlite::params![project],
    )
    .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Copy a group, including columns and incomplete tasks only.
#[tauri::command]
pub fn copy_task_group(project: String) -> Result<GroupData, String> {
    let project = project.trim().to_string();
    if project.is_empty() {
        return Err("Group name cannot be empty".to_string());
    }

    let mut db = get_connection()?;
    let conn = db.as_mut().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    sync_groups_from_columns(&tx)?;

    let source_exists: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM task_columns WHERE project = ?1",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if source_exists == 0 {
        return Err("Group not found".to_string());
    }

    let source_order = group_sort_order(&tx, &project)?;
    tx.execute(
        "UPDATE task_groups SET sort_order = sort_order + 1 WHERE sort_order >= ?1",
        rusqlite::params![source_order],
    )
    .map_err(|e| e.to_string())?;

    let copy_project = next_group_copy_name(&tx, &project)?;
    tx.execute(
        "INSERT INTO task_groups (project, sort_order) VALUES (?1, ?2)",
        rusqlite::params![&copy_project, source_order],
    )
    .map_err(|e| e.to_string())?;
    let source_columns = load_columns_for_project(&tx, &project)?;
    let mut copied_columns = Vec::new();

    for column in source_columns {
        let copied_column_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                &copied_column_id,
                &copy_project,
                &column.name,
                column.sort_order
            ],
        )
        .map_err(|e| e.to_string())?;

        let copied_column = tx
            .query_row(
                "SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns WHERE id = ?1",
                rusqlite::params![copied_column_id],
                column_from_row,
            )
            .map_err(|e| e.to_string())?;

        let source_tasks = load_incomplete_tasks_for_column(&tx, &column.id)?;
        let mut copied_tasks = Vec::new();
        for (position, task) in source_tasks.into_iter().enumerate() {
            let copied_task_id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO tasks (id, project, title, done, progress, column_id, position) VALUES (?1, ?2, ?3, 0, 0, ?4, ?5)",
                rusqlite::params![
                    &copied_task_id,
                    &copy_project,
                    &task.title,
                    &copied_column_id,
                    position as i64
                ],
            )
            .map_err(|e| e.to_string())?;

            let copied_task = tx
                .query_row(
                    &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
                    rusqlite::params![copied_task_id],
                    task_from_row,
                )
                .map_err(|e| e.to_string())?;
            copied_tasks.push(copied_task);
        }

        copied_columns.push(ColumnWithTasks {
            column: copied_column,
            tasks: copied_tasks,
        });
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(GroupData {
        project: copy_project,
        sort_order: source_order,
        columns: copied_columns,
    })
}

/// Rename a non-default task group.
#[tauri::command]
pub fn rename_task_group(project: String, name: String) -> Result<GroupData, String> {
    let project = project.trim().to_string();
    let name = name.trim().to_string();
    if project == "default" {
        return Err("DEFAULT_GROUP".to_string());
    }
    if name.is_empty() {
        return Err("Group name cannot be empty".to_string());
    }
    if project == name {
        return get_board(None)?
            .into_iter()
            .find(|group| group.project == project)
            .ok_or_else(|| "Group not found".to_string());
    }

    {
        let mut db = get_connection()?;
        let conn = db.as_mut().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        sync_groups_from_columns(&tx)?;

        if !group_exists(&tx, &project)? {
            return Err("Group not found".to_string());
        }
        if group_exists(&tx, &name)? {
            return Err("GROUP_EXISTS".to_string());
        }

        tx.execute(
            "UPDATE task_columns SET project = ?1, updated_at = datetime('now') WHERE project = ?2",
            rusqlite::params![name, project],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE task_groups SET project = ?1, updated_at = datetime('now') WHERE project = ?2",
            rusqlite::params![name, project],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE tasks SET project = ?1, updated_at = datetime('now') WHERE project = ?2",
            rusqlite::params![name, project],
        )
        .map_err(|e| e.to_string())?;

        tx.commit().map_err(|e| e.to_string())?
    };

    get_board(None)?
        .into_iter()
        .find(|group| group.project == name)
        .ok_or_else(|| "Group not found".to_string())
}

/// Reorder task groups. Receives ordered list of project names.
#[tauri::command]
pub fn reorder_task_groups(projects: Vec<String>) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    sync_groups_from_columns(conn)?;
    for (i, project) in projects.iter().enumerate() {
        conn.execute(
            "UPDATE task_groups SET sort_order = ?1, updated_at = datetime('now') WHERE project = ?2",
            rusqlite::params![i as i64, project],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Add a task to a specific group's first column.
#[tauri::command]
pub fn add_task_to_group(title: String, project: String) -> Result<Task, String> {
    add_task(title, Some(project))
}

/// Add a task to a specific column.
#[tauri::command]
pub fn add_task_to_column(title: String, column_id: String) -> Result<Task, String> {
    let id = uuid::Uuid::new_v4().to_string();

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    // Get the project for this column
    let project: String = conn
        .query_row(
            "SELECT project FROM task_columns WHERE id = ?1",
            rusqlite::params![column_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Column not found: {}", e))?;

    // Calculate position (append at end)
    let max_pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM tasks WHERE column_id = ?1",
            rusqlite::params![column_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO tasks (id, project, title, progress, column_id, position) VALUES (?1, ?2, ?3, 0, ?4, ?5)",
        rusqlite::params![id, project, title, column_id, max_pos],
    )
    .map_err(|e| e.to_string())?;

    load_task(conn, &id)
}

/// Update task content (inline edit).
#[tauri::command]
pub fn update_task_content(id: String, title: String) -> Result<Task, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE tasks SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![title, id],
    )
    .map_err(|e| e.to_string())?;

    conn.query_row(
        &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
        rusqlite::params![id],
        task_from_row,
    )
    .map_err(|e| e.to_string())
}

/// Update task progress. Syncs done field: progress=100 → done=true, else done=false.
#[tauri::command]
pub fn update_task_progress(id: String, progress: i32) -> Result<Task, String> {
    let progress = progress.clamp(0, 100);
    let done = if progress >= 100 { 1 } else { 0 };
    let completed_sql = if done == 1 {
        "completed_at = COALESCE(completed_at, datetime('now'))"
    } else {
        "completed_at = NULL"
    };

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        &format!("UPDATE tasks SET progress = ?1, done = ?2, {completed_sql}, updated_at = datetime('now') WHERE id = ?3"),
        rusqlite::params![progress, done, id],
    )
    .map_err(|e| e.to_string())?;

    conn.query_row(
        &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
        rusqlite::params![id],
        task_from_row,
    )
    .map_err(|e| e.to_string())
}

/// Delete a task and return its snapshot for undo.
#[tauri::command]
pub fn delete_task_with_snapshot(id: String) -> Result<TaskSnapshot, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    // Get task snapshot before deletion
    let task = conn
        .query_row(
            &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
            rusqlite::params![id],
            task_from_row,
        )
        .map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;

    Ok(TaskSnapshot { task })
}

/// Restore a deleted task from snapshot.
#[tauri::command]
pub fn restore_task(snapshot: TaskSnapshot) -> Result<Task, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let t = &snapshot.task;
    conn.execute(
        "INSERT INTO tasks (id, project, title, done, progress, column_id, position, created_at, updated_at, scheduled_start_at, scheduled_end_at, reminder_minutes, completed_at, repeat_type, recurrence_series_id, recurrence_sequence, recurrence_origin_at, recurrence_detached) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![t.id, t.project, t.title, t.done, t.progress, t.column_id, t.position, t.created_at, t.updated_at, t.scheduled_start_at, t.scheduled_end_at, t.reminder_minutes, t.completed_at, t.repeat_type, t.recurrence_series_id, t.recurrence_sequence, t.recurrence_origin_at, t.recurrence_detached],
    )
    .map_err(|e| e.to_string())?;
    Ok(snapshot.task)
}

#[tauri::command]
pub fn bulk_update_tasks_done(ids: Vec<String>, done: bool) -> Result<Vec<Task>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let completed_at = if done {
        Some(
            Local::now()
                .naive_local()
                .format("%Y-%m-%dT%H:%M:%S")
                .to_string(),
        )
    } else {
        None
    };
    for id in &ids {
        conn.execute(
            "UPDATE tasks SET done = ?1, progress = ?2, completed_at = ?3, updated_at = datetime('now') WHERE id = ?4",
            rusqlite::params![done, if done { 100 } else { 0 }, completed_at, id],
        )
        .map_err(|e| e.to_string())?;
    }
    ids.into_iter().map(|id| load_task(conn, &id)).collect()
}

#[tauri::command]
pub fn bulk_delete_tasks_with_snapshot(ids: Vec<String>) -> Result<Vec<TaskSnapshot>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let mut snapshots = Vec::new();
    let mut affected_columns = std::collections::HashSet::new();
    for id in ids {
        let task = load_task(conn, &id)?;
        affected_columns.insert(task.column_id.clone());
        conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| e.to_string())?;
        snapshots.push(TaskSnapshot { task });
    }
    for column_id in affected_columns {
        if !column_id.is_empty() {
            normalize_positions(conn, &column_id)?;
        }
    }
    Ok(snapshots)
}

#[tauri::command]
pub fn restore_tasks(snapshots: Vec<TaskSnapshot>) -> Result<Vec<Task>, String> {
    let mut restored = Vec::new();
    for snapshot in snapshots {
        restored.push(restore_task(snapshot)?);
    }
    Ok(restored)
}

#[tauri::command]
pub fn bulk_move_tasks(ids: Vec<String>, target_column_id: String) -> Result<Vec<Task>, String> {
    let start_position = {
        let db = get_connection()?;
        let conn = db.as_ref().unwrap();
        next_task_position(conn, &target_column_id)?
    };
    let mut moved = Vec::new();
    for (offset, id) in ids.into_iter().enumerate() {
        let task = move_task(id, target_column_id.clone(), start_position + offset as i64)?;
        moved.push(task);
    }
    Ok(moved)
}

/// Move a task within or across groups.
/// Returns the updated task.
#[tauri::command]
pub fn move_task(
    id: String,
    target_column_id: String,
    target_position: i64,
) -> Result<Task, String> {
    let mut db = get_connection()?;
    let conn = db.as_mut().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let target_project: String = tx
        .query_row(
            "SELECT project FROM task_columns WHERE id = ?1",
            rusqlite::params![target_column_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // Get current column and position
    let (old_column_id, old_position): (String, i64) = tx
        .query_row(
            "SELECT column_id, position FROM tasks WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    // Shift tasks in old column (remove gap)
    if old_column_id != target_column_id {
        tx.execute(
            "UPDATE tasks SET position = position - 1 WHERE column_id = ?1 AND position > ?2",
            rusqlite::params![old_column_id, old_position],
        )
        .map_err(|e| e.to_string())?;
    }

    // Shift tasks in target column (make room)
    tx.execute(
        "UPDATE tasks SET position = position + 1 WHERE column_id = ?1 AND position >= ?2 AND id != ?3",
        rusqlite::params![target_column_id, target_position, id],
    )
    .map_err(|e| e.to_string())?;

    // Move the task
    tx.execute(
        "UPDATE tasks SET project = ?1, column_id = ?2, position = ?3, updated_at = datetime('now') WHERE id = ?4",
        rusqlite::params![&target_project, &target_column_id, target_position, &id],
    )
    .map_err(|e| e.to_string())?;

    // Normalize positions in affected columns
    normalize_positions(&tx, &old_column_id)?;
    if old_column_id != target_column_id {
        normalize_positions(&tx, &target_column_id)?;
    }

    let task = tx
        .query_row(
            &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
            rusqlite::params![id],
            task_from_row,
        )
        .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    Ok(task)
}

/// Create a new column by dragging a task to the drop zone.
/// Returns the new column with the task moved into it.
#[tauri::command]
pub fn create_column_by_drag(
    task_id: String,
    project: String,
) -> Result<(TaskColumn, Task), String> {
    let mut db = get_connection()?;
    let conn = db.as_mut().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (_task_project, old_column_id, old_position): (String, String, i64) = tx
        .query_row(
            "SELECT project, column_id, position FROM tasks WHERE id = ?1",
            rusqlite::params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    // Get max sort order
    let max_order: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM task_columns WHERE project = ?1",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let col_id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, ?2, '新分列', ?3)",
        rusqlite::params![&col_id, &project, max_order],
    )
    .map_err(|e| e.to_string())?;

    // Move task to new column at position 0
    tx.execute(
        "UPDATE tasks SET project = ?1, column_id = ?2, position = 0, updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![&project, &col_id, &task_id],
    )
    .map_err(|e| e.to_string())?;

    // Fix gap in old column
    tx.execute(
        "UPDATE tasks SET position = position - 1 WHERE column_id = ?1 AND position > ?2",
        rusqlite::params![old_column_id, old_position],
    )
    .map_err(|e| e.to_string())?;
    normalize_positions(&tx, &old_column_id)?;

    let col = tx
        .query_row(
            "SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns WHERE id = ?1",
            rusqlite::params![col_id],
            |row| {
                Ok(TaskColumn {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    name: row.get(2)?,
                    sort_order: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let task = tx
        .query_row(
            &format!("SELECT {TASK_SELECT} FROM tasks WHERE id = ?1"),
            rusqlite::params![task_id],
            task_from_row,
        )
        .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    Ok((col, task))
}

/// Create an empty column in a group.
#[tauri::command]
pub fn create_column(project: String, name: Option<String>) -> Result<TaskColumn, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let column_name = name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "新分列".to_string());

    let max_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM task_columns WHERE project = ?1",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let col_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![&col_id, &project, column_name, max_order],
    )
    .map_err(|e| e.to_string())?;

    let column = conn
        .query_row(
            "SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns WHERE id = ?1",
            rusqlite::params![col_id],
            column_from_row,
        )
        .map_err(|e| e.to_string())?;
    let payload = serde_json::to_string(&column).map_err(|e| e.to_string())?;
    changelog::record_change(conn, "task_columns", &column.id, OP_INSERT, &payload)?;
    Ok(column)
}

/// Rename a column.
#[tauri::command]
pub fn rename_column(id: String, name: String) -> Result<TaskColumn, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE task_columns SET name = ?1, updated_at = datetime('now') WHERE id = ?2",
        rusqlite::params![name, id],
    )
    .map_err(|e| e.to_string())?;

    conn.query_row(
        "SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(TaskColumn {
                id: row.get(0)?,
                project: row.get(1)?,
                name: row.get(2)?,
                sort_order: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

/// Reorder columns within a group. Receives ordered list of column IDs.
#[tauri::command]
pub fn reorder_columns(column_ids: Vec<String>) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    for (i, col_id) in column_ids.iter().enumerate() {
        conn.execute(
            "UPDATE task_columns SET sort_order = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![i as i64, col_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Delete an empty column. Returns snapshot for undo.
/// Fails if column is non-empty or is the last column in its group.
#[tauri::command]
pub fn delete_column(id: String) -> Result<ColumnSnapshot, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    // Check if column has tasks
    let task_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE column_id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if task_count > 0 {
        return Err("NON_EMPTY".to_string());
    }

    // Get column info
    let col = conn
        .query_row(
            "SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(TaskColumn {
                    id: row.get(0)?,
                    project: row.get(1)?,
                    name: row.get(2)?,
                    sort_order: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    // Check if it's the last column in the group
    let col_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_columns WHERE project = ?1",
            rusqlite::params![col.project],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    if col_count <= 1 {
        return Err("LAST_COLUMN".to_string());
    }

    conn.execute(
        "DELETE FROM task_columns WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;

    // Reorder remaining columns
    let mut stmt = conn
        .prepare("SELECT id FROM task_columns WHERE project = ?1 ORDER BY sort_order ASC")
        .map_err(|e| e.to_string())?;
    let remaining: Vec<String> = stmt
        .query_map(rusqlite::params![col.project], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    for (i, cid) in remaining.iter().enumerate() {
        let _ = conn.execute(
            "UPDATE task_columns SET sort_order = ?1 WHERE id = ?2",
            rusqlite::params![i as i64, cid],
        );
    }

    Ok(ColumnSnapshot { column: col })
}

/// Restore a deleted column from snapshot.
#[tauri::command]
pub fn restore_column(snapshot: ColumnSnapshot) -> Result<TaskColumn, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let c = &snapshot.column;
    conn.execute(
        "INSERT INTO task_columns (id, project, name, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![c.id, c.project, c.name, c.sort_order, c.created_at, c.updated_at],
    )
    .map_err(|e| e.to_string())?;
    Ok(snapshot.column)
}

// ---------- Calendar task commands ----------

#[tauri::command]
pub fn get_calendar_tasks(
    start_at: Option<String>,
    end_at: Option<String>,
) -> Result<Vec<Task>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let sql = match (&start_at, &end_at) {
        (Some(_), Some(_)) => format!(
            "SELECT {TASK_SELECT} FROM tasks
             WHERE scheduled_start_at IS NOT NULL
               AND COALESCE(scheduled_end_at, scheduled_start_at) >= ?1
               AND scheduled_start_at <= ?2
             ORDER BY scheduled_start_at ASC, position ASC"
        ),
        _ => format!(
            "SELECT {TASK_SELECT} FROM tasks
             WHERE scheduled_start_at IS NOT NULL
             ORDER BY scheduled_start_at ASC, position ASC"
        ),
    };

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = match (start_at, end_at) {
        (Some(start), Some(end)) => stmt
            .query_map(rusqlite::params![start, end], task_from_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>(),
        _ => stmt
            .query_map([], task_from_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>(),
    }
    .map_err(|e| e.to_string())?;

    Ok(rows)
}

#[tauri::command]
pub fn create_calendar_task(input: CalendarTaskInput) -> Result<Task, String> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err("TITLE_REQUIRED".to_string());
    }
    let repeat_type = normalize_repeat_type(input.repeat_type)?;
    let reminder_minutes = input.reminder_minutes.unwrap_or(0).max(0);
    let project = input
        .project
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| project_from_start(&input.scheduled_start_at));
    let done = input.done.unwrap_or(false);

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let column_id = match input.column_id {
        Some(column_id)
            if !column_id.trim().is_empty()
                && column_belongs_to_project(conn, &column_id, &project)? =>
        {
            column_id
        }
        _ => ensure_group_and_columns(conn, &project)?,
    };
    let position = next_task_position(conn, &column_id)?;
    let id = uuid::Uuid::new_v4().to_string();
    let series_id = if repeat_type == "none" {
        None
    } else {
        Some(uuid::Uuid::new_v4().to_string())
    };

    if let Some(series_id) = &series_id {
        conn.execute(
            "INSERT INTO recurrence_series (id, repeat_type, title, start_at, end_at, active) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            rusqlite::params![series_id, repeat_type, title, input.scheduled_start_at, input.scheduled_end_at],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.execute(
        "INSERT INTO tasks (id, project, title, done, progress, column_id, position, scheduled_start_at, scheduled_end_at, reminder_minutes, completed_at, repeat_type, recurrence_series_id, recurrence_sequence, recurrence_origin_at, recurrence_detached)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, 0)",
        rusqlite::params![
            id,
            project,
            title,
            done,
            if done { 100 } else { 0 },
            column_id,
            position,
            input.scheduled_start_at,
            input.scheduled_end_at,
            reminder_minutes,
            if done { Some(Local::now().naive_local().format("%Y-%m-%dT%H:%M:%S").to_string()) } else { None },
            repeat_type,
            series_id,
            if series_id.is_some() { Some(0_i64) } else { None },
            if series_id.is_some() { Some(input.scheduled_start_at.clone()) } else { None },
        ],
    )
    .map_err(|e| e.to_string())?;

    if let Some(series_id) = &series_id {
        generate_instances_for_series(conn, series_id, &id)?;
    }

    load_task(conn, &id)
}

#[tauri::command]
pub fn update_calendar_task(
    id: String,
    input: CalendarTaskInput,
    scope: Option<String>,
) -> Result<Task, String> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err("TITLE_REQUIRED".to_string());
    }
    let repeat_type = normalize_repeat_type(input.repeat_type)?;
    let reminder_minutes = input.reminder_minutes.unwrap_or(0).max(0);
    let project = input
        .project
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| project_from_start(&input.scheduled_start_at));

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let task = load_task(conn, &id)?;
    let column_id = match input.column_id {
        Some(column_id)
            if !column_id.trim().is_empty()
                && column_belongs_to_project(conn, &column_id, &project)? =>
        {
            column_id
        }
        _ if task.project == project
            && !task.column_id.is_empty()
            && column_belongs_to_project(conn, &task.column_id, &project)? =>
        {
            task.column_id
        }
        _ => ensure_group_and_columns(conn, &project)?,
    };

    let series_id = if repeat_type == "none" {
        None
    } else {
        task.recurrence_series_id
            .clone()
            .or_else(|| Some(uuid::Uuid::new_v4().to_string()))
    };
    let sequence = if series_id.is_some() {
        task.recurrence_sequence.or(Some(0))
    } else {
        None
    };
    let origin_at = if series_id.is_some() {
        task.recurrence_origin_at
            .clone()
            .or_else(|| Some(input.scheduled_start_at.clone()))
    } else {
        None
    };

    if let Some(series_id) = &series_id {
        conn.execute(
            "INSERT OR REPLACE INTO recurrence_series (id, repeat_type, title, start_at, end_at, active, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 1, datetime('now'))",
            rusqlite::params![series_id, repeat_type, title, input.scheduled_start_at, input.scheduled_end_at],
        )
        .map_err(|e| e.to_string())?;
    }

    let done = input.done.unwrap_or(task.done);
    let recurrence_detached =
        task.recurrence_series_id.is_some() && scope.as_deref() == Some("single");
    conn.execute(
        "UPDATE tasks
         SET title = ?1, project = ?2, column_id = ?3, scheduled_start_at = ?4, scheduled_end_at = ?5,
             reminder_minutes = ?6, repeat_type = ?7, recurrence_series_id = ?8, recurrence_sequence = ?9,
             recurrence_origin_at = ?10, recurrence_detached = ?11, done = ?12, progress = ?13,
             completed_at = CASE WHEN ?12 = 1 THEN COALESCE(completed_at, datetime('now')) ELSE NULL END,
             updated_at = datetime('now')
         WHERE id = ?14",
        rusqlite::params![
            title,
            project,
            column_id,
            input.scheduled_start_at,
            input.scheduled_end_at,
            reminder_minutes,
            repeat_type,
            series_id,
            sequence,
            origin_at,
            recurrence_detached,
            done,
            if done { 100 } else { 0 },
            id,
        ],
    )
    .map_err(|e| e.to_string())?;

    if repeat_type != "none"
        && let Some(series_id) = &series_id
    {
        if scope.as_deref() == Some("future") {
            delete_future_instances(conn, series_id, sequence.unwrap_or(0) + 1)?;
        }
        generate_instances_for_series(conn, series_id, &id)?;
    }

    load_task(conn, &id)
}

#[tauri::command]
pub fn remove_task_from_schedule(id: String, scope: Option<String>) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let task = load_task(conn, &id)?;
    if scope.as_deref() == Some("future")
        && let Some(series_id) = task.recurrence_series_id
    {
        conn.execute(
            "UPDATE tasks SET scheduled_start_at = NULL, scheduled_end_at = NULL, reminder_minutes = 0,
                 repeat_type = 'none', recurrence_series_id = NULL, recurrence_sequence = NULL,
                 recurrence_origin_at = NULL, recurrence_detached = 1, updated_at = datetime('now')
                 WHERE recurrence_series_id = ?1 AND COALESCE(recurrence_sequence,0) >= ?2",
            rusqlite::params![series_id, task.recurrence_sequence.unwrap_or(0)],
        )
        .map_err(|e| e.to_string())?;
        deactivate_series(conn, &series_id)?;
        return Ok(());
    }
    conn.execute(
        "UPDATE tasks SET scheduled_start_at = NULL, scheduled_end_at = NULL, reminder_minutes = 0,
         repeat_type = 'none', recurrence_series_id = NULL, recurrence_sequence = NULL,
         recurrence_origin_at = NULL, recurrence_detached = 1, updated_at = datetime('now')
         WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cancel_task_recurrence(id: String, scope: String) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let task = load_task(conn, &id)?;
    if scope == "future" {
        if let Some(series_id) = task.recurrence_series_id {
            conn.execute(
                "UPDATE tasks SET repeat_type = 'none', recurrence_series_id = NULL, recurrence_sequence = NULL,
                 recurrence_origin_at = NULL, recurrence_detached = 1, updated_at = datetime('now')
                 WHERE recurrence_series_id = ?1 AND COALESCE(recurrence_sequence,0) >= ?2",
                rusqlite::params![series_id, task.recurrence_sequence.unwrap_or(0)],
            )
            .map_err(|e| e.to_string())?;
            deactivate_series(conn, &series_id)?;
        }
    } else {
        conn.execute(
            "UPDATE tasks SET repeat_type = 'none', recurrence_series_id = NULL, recurrence_sequence = NULL,
             recurrence_origin_at = NULL, recurrence_detached = 1, updated_at = datetime('now')
             WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn delete_recurring_tasks(id: String, scope: String) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let task = load_task(conn, &id)?;
    if scope == "future"
        && let Some(series_id) = task.recurrence_series_id
    {
        conn.execute(
            "DELETE FROM tasks WHERE recurrence_series_id = ?1 AND COALESCE(recurrence_sequence,0) >= ?2",
            rusqlite::params![series_id, task.recurrence_sequence.unwrap_or(0)],
        )
        .map_err(|e| e.to_string())?;
        deactivate_series(conn, &series_id)?;
        return Ok(());
    }
    conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn ensure_recurring_task_instances() -> Result<usize, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let mut stmt = conn
        .prepare("SELECT id FROM recurrence_series WHERE COALESCE(active, 1) = 1")
        .map_err(|e| e.to_string())?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut created = 0;
    for series_id in ids {
        if let Ok(root_id) = conn.query_row(
            "SELECT id FROM tasks WHERE recurrence_series_id = ?1 ORDER BY COALESCE(recurrence_sequence,0) ASC LIMIT 1",
            rusqlite::params![series_id],
            |row| row.get::<_, String>(0),
        ) {
            created += generate_instances_for_series(conn, &series_id, &root_id)?;
        }
    }
    Ok(created)
}

// ---------- Helpers ----------

fn next_task_position(conn: &rusqlite::Connection, column_id: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM tasks WHERE column_id = ?1",
        rusqlite::params![column_id],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

fn parse_task_datetime(value: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M"))
        .map_err(|_| "INVALID_DATETIME".to_string())
}

fn format_task_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn add_months_clamped(value: NaiveDateTime, months: i32) -> NaiveDateTime {
    let base_month = value.month() as i32 - 1 + months;
    let year = value.year() + base_month.div_euclid(12);
    let month = base_month.rem_euclid(12) as u32 + 1;
    let last_day = last_day_of_month(year, month);
    let day = value.day().min(last_day);
    NaiveDate::from_ymd_opt(year, month, day)
        .map(|date| date.and_time(value.time()))
        .unwrap_or(value)
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|date| date.pred_opt())
        .map(|date| date.day())
        .unwrap_or(28)
}

fn next_recurrence(value: NaiveDateTime, repeat_type: &str, sequence: i64) -> NaiveDateTime {
    match repeat_type {
        "daily" => value + Duration::days(sequence),
        "weekly" => value + Duration::weeks(sequence),
        "monthly" => add_months_clamped(value, sequence as i32),
        "yearly" => add_months_clamped(value, (sequence * 12) as i32),
        _ => value,
    }
}

fn delete_future_instances(
    conn: &rusqlite::Connection,
    series_id: &str,
    min_sequence: i64,
) -> Result<(), String> {
    conn.execute(
        "DELETE FROM tasks WHERE recurrence_series_id = ?1 AND COALESCE(recurrence_sequence,0) >= ?2",
        rusqlite::params![series_id, min_sequence],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn deactivate_series(conn: &rusqlite::Connection, series_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE recurrence_series SET active = 0, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![series_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn generate_instances_for_series(
    conn: &rusqlite::Connection,
    series_id: &str,
    root_id: &str,
) -> Result<usize, String> {
    let active: bool = conn
        .query_row(
            "SELECT COALESCE(active, 1) FROM recurrence_series WHERE id = ?1",
            rusqlite::params![series_id],
            |row| Ok(row.get::<_, i32>(0)? != 0),
        )
        .unwrap_or(true);
    if !active {
        return Ok(0);
    }

    let root = load_task(conn, root_id)?;
    let repeat_type = root.repeat_type.clone();
    if repeat_type == "none" {
        return Ok(0);
    }
    let start_at = root
        .scheduled_start_at
        .as_deref()
        .ok_or_else(|| "MISSING_START_AT".to_string())
        .and_then(parse_task_datetime)?;
    let end_at = root
        .scheduled_end_at
        .as_deref()
        .ok_or_else(|| "MISSING_END_AT".to_string())
        .and_then(parse_task_datetime)?;
    let duration = end_at - start_at;
    let max_sequence: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(COALESCE(recurrence_sequence, 0)), 0) FROM tasks WHERE recurrence_series_id = ?1",
            rusqlite::params![series_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if max_sequence > 0 {
        let last_start: Option<String> = conn
            .query_row(
                "SELECT scheduled_start_at FROM tasks WHERE recurrence_series_id = ?1 AND recurrence_sequence = ?2 LIMIT 1",
                rusqlite::params![series_id, max_sequence],
                |row| row.get(0),
            )
            .ok();
        if let Some(last_start) = last_start {
            let refill_after =
                Local::now().naive_local() + Duration::days(RECURRENCE_REFILL_THRESHOLD_DAYS);
            if parse_task_datetime(&last_start)? > refill_after {
                return Ok(0);
            }
        }
    }

    let initial_until = start_at + Duration::days(RECURRENCE_WINDOW_DAYS);
    let rolling_until = Local::now().naive_local() + Duration::days(RECURRENCE_WINDOW_DAYS);
    let until = if rolling_until > initial_until {
        rolling_until
    } else {
        initial_until
    };
    let start_sequence = max_sequence + 1;
    let end_sequence = start_sequence + RECURRENCE_BATCH_LIMIT;
    let mut created = 0;

    for sequence in start_sequence..end_sequence {
        let occurrence_start = next_recurrence(start_at, &repeat_type, sequence);
        if occurrence_start > until {
            break;
        }
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE recurrence_series_id = ?1 AND recurrence_sequence = ?2",
                rusqlite::params![series_id, sequence],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            continue;
        }

        let project = project_from_start(&format_task_datetime(occurrence_start));
        let column_id = ensure_group_and_columns(conn, &project)?;
        let position = next_task_position(conn, &column_id)?;
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tasks (id, project, title, done, progress, column_id, position, scheduled_start_at, scheduled_end_at, reminder_minutes, repeat_type, recurrence_series_id, recurrence_sequence, recurrence_origin_at, recurrence_detached)
             VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0)",
            rusqlite::params![
                id,
                project,
                root.title,
                column_id,
                position,
                format_task_datetime(occurrence_start),
                format_task_datetime(occurrence_start + duration),
                root.reminder_minutes,
                repeat_type,
                series_id,
                sequence,
                format_task_datetime(occurrence_start),
            ],
        )
        .map_err(|e| e.to_string())?;
        created += 1;
    }

    Ok(created)
}

fn task_from_row(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        project: row.get(1)?,
        title: row.get(2)?,
        done: row.get::<_, i32>(3)? != 0,
        progress: row.get(4)?,
        column_id: row.get(5)?,
        position: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        scheduled_start_at: row.get(9)?,
        scheduled_end_at: row.get(10)?,
        reminder_minutes: row.get(11)?,
        completed_at: row.get(12)?,
        repeat_type: row.get(13)?,
        recurrence_series_id: row.get(14)?,
        recurrence_sequence: row.get(15)?,
        recurrence_origin_at: row.get(16)?,
        recurrence_detached: row.get::<_, i32>(17)? != 0,
    })
}

fn column_from_row(row: &rusqlite::Row) -> rusqlite::Result<TaskColumn> {
    Ok(TaskColumn {
        id: row.get(0)?,
        project: row.get(1)?,
        name: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn load_columns_for_project(
    conn: &rusqlite::Connection,
    project: &str,
) -> Result<Vec<TaskColumn>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns WHERE project = ?1 ORDER BY sort_order ASC",
        )
        .map_err(|e| e.to_string())?;
    stmt.query_map(rusqlite::params![project], column_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn load_incomplete_tasks_for_column(
    conn: &rusqlite::Connection,
    column_id: &str,
) -> Result<Vec<Task>, String> {
    let mut stmt = conn
        .prepare(
            &format!("SELECT {TASK_SELECT} FROM tasks WHERE column_id = ?1 AND done = 0 ORDER BY position ASC, created_at DESC"),
        )
        .map_err(|e| e.to_string())?;
    stmt.query_map(rusqlite::params![column_id], task_from_row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn group_exists(conn: &rusqlite::Connection, project: &str) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_columns WHERE project = ?1",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

fn group_sort_order(conn: &rusqlite::Connection, project: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT sort_order FROM task_groups WHERE project = ?1",
        rusqlite::params![project],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

fn next_group_sort_order(conn: &rusqlite::Connection) -> Result<i64, String> {
    conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM task_groups",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

fn sync_groups_from_columns(conn: &rusqlite::Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT project FROM task_columns GROUP BY project ORDER BY project ASC")
        .map_err(|e| e.to_string())?;
    let projects: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    for project in projects {
        let next_order = next_group_sort_order(conn)?;
        conn.execute(
            "INSERT OR IGNORE INTO task_groups (project, sort_order) VALUES (?1, ?2)",
            rusqlite::params![project, next_order],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn repair_board_task_placement(conn: &rusqlite::Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id, project, COALESCE(column_id,''), scheduled_start_at FROM tasks")
        .map_err(|e| e.to_string())?;
    let task_rows: Vec<(String, String, String, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|row| row.ok())
        .collect();

    for (id, project, column_id, scheduled_start_at) in task_rows {
        let target_project = scheduled_start_at
            .as_deref()
            .map(project_from_start)
            .unwrap_or_else(|| {
                if project.trim().is_empty() {
                    today_project()
                } else {
                    project.clone()
                }
            });
        let column_project: Option<String> = if column_id.is_empty() {
            None
        } else {
            conn.query_row(
                "SELECT project FROM task_columns WHERE id = ?1",
                rusqlite::params![column_id],
                |row| row.get(0),
            )
            .ok()
        };

        if project == target_project && column_project.as_deref() == Some(target_project.as_str()) {
            continue;
        }

        let target_column_id = ensure_group_and_columns(conn, &target_project)?;
        let position = next_task_position(conn, &target_column_id)?;
        conn.execute(
            "UPDATE tasks SET project = ?1, column_id = ?2, position = ?3, updated_at = datetime('now') WHERE id = ?4",
            rusqlite::params![target_project, target_column_id, position, id],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn next_group_copy_name(conn: &rusqlite::Connection, project: &str) -> Result<String, String> {
    let base = if project == "default" {
        "默认分组 副本".to_string()
    } else {
        format!("{project} 副本")
    };

    if !group_exists(conn, &base)? {
        return Ok(base);
    }

    for index in 2.. {
        let candidate = format!("{base} {index}");
        if !group_exists(conn, &candidate)? {
            return Ok(candidate);
        }
    }

    unreachable!()
}

fn normalize_positions(conn: &rusqlite::Connection, column_id: &str) -> Result<(), String> {
    let mut stmt = conn
        .prepare("SELECT id FROM tasks WHERE column_id = ?1 ORDER BY position ASC, created_at DESC")
        .map_err(|e| e.to_string())?;
    let ids: Vec<String> = stmt
        .query_map(rusqlite::params![column_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    for (i, tid) in ids.iter().enumerate() {
        let _ = conn.execute(
            "UPDATE tasks SET position = ?1 WHERE id = ?2",
            rusqlite::params![i as i64, tid],
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};

    static TEST_DB_LOCK: Mutex<()> = Mutex::new(());

    fn setup_test_db() -> (MutexGuard<'static, ()>, PathBuf) {
        let guard = TEST_DB_LOCK.lock().unwrap();
        let path = std::env::temp_dir().join(format!("nalu-tasks-{}.sqlite", uuid::Uuid::new_v4()));
        crate::db::database::init(&path).unwrap();
        (guard, path)
    }

    fn group_column_count(project: &str) -> usize {
        get_board(None)
            .unwrap()
            .into_iter()
            .find(|group| group.project == project)
            .map(|group| group.columns.len())
            .unwrap_or(0)
    }

    #[test]
    fn create_column_by_drag_moves_task_into_new_column_in_another_group() {
        let (_guard, path) = setup_test_db();
        let task = add_task("default task".to_string(), Some("default".to_string())).unwrap();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();
        let side_columns_before = group_column_count("side");

        let (column, moved_task) = create_column_by_drag(task.id, "side".to_string()).unwrap();

        assert_eq!(column.project, "side");
        assert_eq!(moved_task.project, "side");
        assert_eq!(moved_task.column_id, column.id);
        assert_eq!(moved_task.position, 0);
        assert_eq!(group_column_count("side"), side_columns_before + 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn create_column_by_drag_moves_task_into_new_column_in_same_group() {
        let (_guard, path) = setup_test_db();
        let task = add_task("default task".to_string(), Some("default".to_string())).unwrap();
        let default_columns_before = group_column_count("default");

        let (column, moved_task) =
            create_column_by_drag(task.id.clone(), "default".to_string()).unwrap();

        assert_eq!(column.project, "default");
        assert_eq!(column.name, "新分列");
        assert_eq!(moved_task.project, "default");
        assert_eq!(moved_task.column_id, column.id);
        assert_eq!(moved_task.position, 0);
        assert_eq!(group_column_count("default"), default_columns_before + 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn move_task_updates_project_when_moved_to_another_group() {
        let (_guard, path) = setup_test_db();
        let task = add_task("default task".to_string(), Some("default".to_string())).unwrap();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();
        let side_column_id = get_board(None)
            .unwrap()
            .into_iter()
            .find(|group| group.project == "side")
            .and_then(|group| group.columns.into_iter().next())
            .map(|column| column.column.id)
            .unwrap();

        let moved_task = move_task(task.id, side_column_id.clone(), 0).unwrap();

        assert_eq!(moved_task.project, "side");
        assert_eq!(moved_task.column_id, side_column_id);
        assert_eq!(moved_task.position, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bulk_update_tasks_done_updates_all_selected_tasks() {
        let (_guard, path) = setup_test_db();
        let first = add_task("first".to_string(), Some("default".to_string())).unwrap();
        let second = add_task("second".to_string(), Some("default".to_string())).unwrap();

        let updated =
            bulk_update_tasks_done(vec![first.id.clone(), second.id.clone()], true).unwrap();

        assert_eq!(updated.len(), 2);
        assert!(updated.iter().all(|task| task.done));
        assert!(updated.iter().all(|task| task.progress == 100));
        assert!(updated.iter().all(|task| task.completed_at.is_some()));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bulk_move_tasks_appends_selected_tasks_to_target_column() {
        let (_guard, path) = setup_test_db();
        let first = add_task("first".to_string(), Some("default".to_string())).unwrap();
        let second = add_task("second".to_string(), Some("default".to_string())).unwrap();
        add_task("side".to_string(), Some("side".to_string())).unwrap();
        let side_column_id = get_board(None)
            .unwrap()
            .into_iter()
            .find(|group| group.project == "side")
            .and_then(|group| group.columns.into_iter().next())
            .map(|column| column.column.id)
            .unwrap();

        let moved = bulk_move_tasks(vec![first.id, second.id], side_column_id.clone()).unwrap();

        assert_eq!(moved.len(), 2);
        assert!(moved.iter().all(|task| task.project == "side"));
        assert!(moved.iter().all(|task| task.column_id == side_column_id));
        assert_eq!(moved[0].position + 1, moved[1].position);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn delete_task_group_rejects_group_with_incomplete_tasks() {
        let (_guard, path) = setup_test_db();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();

        let result = delete_task_group("side".to_string());

        assert_eq!(result.unwrap_err(), "HAS_INCOMPLETE_TASKS");
        assert!(
            get_board(None)
                .unwrap()
                .into_iter()
                .any(|group| group.project == "side")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn delete_task_group_removes_group_when_all_tasks_are_done() {
        let (_guard, path) = setup_test_db();
        let task = add_task("side task".to_string(), Some("side".to_string())).unwrap();
        toggle_task(task.id).unwrap();

        delete_task_group("side".to_string()).unwrap();

        assert!(
            !get_board(None)
                .unwrap()
                .into_iter()
                .any(|group| group.project == "side")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn complete_task_group_marks_incomplete_tasks_done() {
        let (_guard, path) = setup_test_db();
        let open_task = add_task("open task".to_string(), Some("side".to_string())).unwrap();
        let done_task = add_task("done task".to_string(), Some("side".to_string())).unwrap();
        toggle_task(done_task.id.clone()).unwrap();

        let updated = complete_task_group("side".to_string()).unwrap();

        assert_eq!(updated, 1);
        let tasks = get_tasks(Some("side".to_string())).unwrap();
        assert!(tasks.iter().all(|task| task.done));
        assert!(
            tasks
                .iter()
                .find(|task| task.id == open_task.id)
                .is_some_and(|task| task.progress == 100)
        );
        assert!(
            tasks
                .iter()
                .find(|task| task.id == done_task.id)
                .is_some_and(|task| task.progress == 100)
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn completed_task_appears_after_incomplete_tasks_in_column() {
        let (_guard, path) = setup_test_db();
        let first = add_task("first task".to_string(), Some("side".to_string())).unwrap();
        let second = add_task("second task".to_string(), Some("side".to_string())).unwrap();

        toggle_task(first.id.clone()).unwrap();

        let task_titles: Vec<String> = get_board(None)
            .unwrap()
            .into_iter()
            .find(|group| group.project == "side")
            .unwrap()
            .columns
            .into_iter()
            .flat_map(|column| column.tasks.into_iter().map(|task| task.title))
            .collect();
        assert_eq!(task_titles, vec![second.title, first.title]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_board_repairs_scheduled_task_into_date_group() {
        let (_guard, path) = setup_test_db();
        let today = today_project();
        let task_id = uuid::Uuid::new_v4().to_string();
        {
            let db = get_connection().unwrap();
            let conn = db.as_ref().unwrap();
            conn.execute(
                "INSERT INTO tasks (id, project, title, column_id, position, scheduled_start_at, scheduled_end_at)
                 VALUES (?1, 'default', 'scheduled today', '', 0, ?2, ?3)",
                rusqlite::params![
                    task_id,
                    format!("{today}T09:00:00"),
                    format!("{today}T10:00:00")
                ],
            )
            .unwrap();
        }

        let today_group = get_board(None)
            .unwrap()
            .into_iter()
            .find(|group| group.project == today)
            .unwrap();
        let repaired_task = today_group
            .columns
            .into_iter()
            .flat_map(|column| column.tasks)
            .find(|task| task.id == task_id)
            .unwrap();

        assert_eq!(repaired_task.project, today);
        assert!(!repaired_task.column_id.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn get_board_hides_future_empty_recurring_date_groups_by_default() {
        let (_guard, path) = setup_test_db();
        let today = Local::now().date_naive();
        let tomorrow = today + Duration::days(1);
        create_calendar_task(CalendarTaskInput {
            title: "daily recurring".to_string(),
            project: None,
            column_id: None,
            scheduled_start_at: format!("{}T09:00:00", today.format("%Y-%m-%d")),
            scheduled_end_at: format!("{}T10:00:00", today.format("%Y-%m-%d")),
            reminder_minutes: Some(5),
            repeat_type: Some("daily".to_string()),
            done: Some(false),
        })
        .unwrap();

        let default_projects: Vec<String> = get_board(None)
            .unwrap()
            .into_iter()
            .map(|group| group.project)
            .collect();
        let expanded_projects: Vec<String> = get_board(Some(true))
            .unwrap()
            .into_iter()
            .map(|group| group.project)
            .collect();
        let tomorrow_project = tomorrow.format("%Y-%m-%d").to_string();

        assert!(!default_projects.contains(&tomorrow_project));
        assert!(expanded_projects.contains(&tomorrow_project));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn copy_task_group_copies_only_incomplete_tasks() {
        let (_guard, path) = setup_test_db();
        add_task("open task".to_string(), Some("side".to_string())).unwrap();
        let done_task = add_task("done task".to_string(), Some("side".to_string())).unwrap();
        toggle_task(done_task.id).unwrap();

        let copied = copy_task_group("side".to_string()).unwrap();

        assert_eq!(copied.project, "side 副本");
        let copied_titles: Vec<String> = copied
            .columns
            .into_iter()
            .flat_map(|column| column.tasks.into_iter().map(|task| task.title))
            .collect();
        assert_eq!(copied_titles, vec!["open task".to_string()]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn copy_task_group_places_copy_above_source_group() {
        let (_guard, path) = setup_test_db();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();
        get_board(None).unwrap();

        copy_task_group("side".to_string()).unwrap();

        let projects: Vec<String> = get_board(None)
            .unwrap()
            .into_iter()
            .map(|group| group.project)
            .collect();
        let copy_idx = projects
            .iter()
            .position(|project| project == "side 副本")
            .unwrap();
        let source_idx = projects
            .iter()
            .position(|project| project == "side")
            .unwrap();
        assert!(copy_idx < source_idx);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reorder_task_groups_persists_group_order() {
        let (_guard, path) = setup_test_db();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();
        get_board(None).unwrap();

        reorder_task_groups(vec!["side".to_string(), "default".to_string()]).unwrap();

        let projects: Vec<String> = get_board(None)
            .unwrap()
            .into_iter()
            .map(|group| group.project)
            .collect();
        assert_eq!(projects, vec!["side".to_string(), "default".to_string()]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rename_task_group_updates_columns_and_tasks() {
        let (_guard, path) = setup_test_db();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();

        let renamed = rename_task_group("side".to_string(), "renamed".to_string()).unwrap();

        assert_eq!(renamed.project, "renamed");
        assert!(!renamed.columns.is_empty());
        assert_eq!(renamed.columns[0].column.project, "renamed");
        let task_projects: Vec<String> = get_tasks(Some("renamed".to_string()))
            .unwrap()
            .into_iter()
            .map(|task| task.project)
            .collect();
        assert_eq!(task_projects, vec!["renamed".to_string()]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rename_task_group_rejects_existing_group_name() {
        let (_guard, path) = setup_test_db();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();

        let result = rename_task_group("side".to_string(), "default".to_string());

        assert_eq!(result.unwrap_err(), "GROUP_EXISTS");
        let _ = std::fs::remove_file(path);
    }
}
