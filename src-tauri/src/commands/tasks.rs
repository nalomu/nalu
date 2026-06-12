use crate::db::database::get_connection;
use serde::{Deserialize, Serialize};

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

// ---------- Legacy commands (kept for dashboard/AI compatibility) ----------

#[tauri::command]
pub fn get_tasks(project: Option<String>) -> Result<Vec<Task>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let tasks = match &project {
        Some(p) => {
            let mut stmt = conn
                .prepare("SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE project = ?1 ORDER BY created_at DESC")
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
                .prepare("SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks ORDER BY created_at DESC")
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
    let project = project.unwrap_or_else(|| "default".to_string());

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    // Find the first column for this project
    let column_id: String = conn
        .query_row(
            "SELECT id FROM task_columns WHERE project = ?1 ORDER BY sort_order ASC LIMIT 1",
            rusqlite::params![project],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| {
            // Auto-create default columns (紧急, 重要, 一般) if missing
            let mut first_id = String::new();
            for (idx, name) in crate::db::database::DEFAULT_COLUMNS.iter().enumerate() {
                let col_id = uuid::Uuid::new_v4().to_string();
                if idx == 0 { first_id = col_id.clone(); }
                let _ = conn.execute(
                    "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![col_id, project, name, idx as i64],
                );
            }
            first_id
        });

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

    Ok(Task {
        id,
        project,
        title,
        done: false,
        progress: 0,
        column_id,
        position: max_pos,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
pub fn toggle_task(id: String) -> Result<bool, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE tasks SET done = 1 - done, progress = CASE WHEN done = 0 THEN 100 ELSE 0 END, updated_at = datetime('now') WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;

    let done: bool = conn
        .query_row(
            "SELECT done FROM tasks WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, i32>(0),
        )
        .map_err(|e| e.to_string())?
        != 0;

    Ok(done)
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
        "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
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
pub fn get_board() -> Result<Vec<GroupData>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

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
        .prepare("SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks ORDER BY position ASC")
        .map_err(|e| e.to_string())?;
    let all_tasks: Vec<Task> = task_stmt
        .query_map([], task_from_row)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

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

    Ok(group_rows
        .into_iter()
        .map(|(project, sort_order)| GroupData {
            columns: columns_by_group.remove(&project).unwrap_or_default(),
            project,
            sort_order,
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
                    "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
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
        return get_board()?
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

    get_board()?
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

    Ok(Task {
        id,
        project,
        title,
        done: false,
        progress: 0,
        column_id,
        position: max_pos,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    })
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
        "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
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

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE tasks SET progress = ?1, done = ?2, updated_at = datetime('now') WHERE id = ?3",
        rusqlite::params![progress, done, id],
    )
    .map_err(|e| e.to_string())?;

    conn.query_row(
        "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
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
            "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
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
        "INSERT INTO tasks (id, project, title, done, progress, column_id, position, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![t.id, t.project, t.title, t.done, t.progress, t.column_id, t.position, t.created_at, t.updated_at],
    )
    .map_err(|e| e.to_string())?;
    Ok(snapshot.task)
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

    let task = tx.query_row(
        "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
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
            "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
            rusqlite::params![task_id],
            task_from_row,
        )
        .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    Ok((col, task))
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

// ---------- Helpers ----------

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
            "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE column_id = ?1 AND done = 0 ORDER BY position ASC, created_at DESC",
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
        get_board()
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
        let side_column_id = get_board()
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
    fn delete_task_group_rejects_group_with_incomplete_tasks() {
        let (_guard, path) = setup_test_db();
        add_task("side task".to_string(), Some("side".to_string())).unwrap();

        let result = delete_task_group("side".to_string());

        assert_eq!(result.unwrap_err(), "HAS_INCOMPLETE_TASKS");
        assert!(
            get_board()
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
            !get_board()
                .unwrap()
                .into_iter()
                .any(|group| group.project == "side")
        );
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
        get_board().unwrap();

        copy_task_group("side".to_string()).unwrap();

        let projects: Vec<String> = get_board()
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
        get_board().unwrap();

        reorder_task_groups(vec!["side".to_string(), "default".to_string()]).unwrap();

        let projects: Vec<String> = get_board()
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
