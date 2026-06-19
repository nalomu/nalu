use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
};
use nalu_shared::models::{ColumnWithTasks, GroupData, Task, TaskColumn};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::{self, SharedState};

#[derive(Debug, Deserialize)]
pub struct AddTaskRequest {
    pub title: String,
    pub project: Option<String>,
    pub column_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub title: Option<String>,
    pub done: Option<bool>,
    pub progress: Option<i32>,
}

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/tasks", get(get_tasks).post(add_task))
        .route("/tasks/{id}", put(update_task).delete(delete_task_handler))
}

async fn get_tasks(
    State(state): State<SharedState>,
) -> Result<Json<Vec<GroupData>>, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;

    let mut group_stmt = conn
        .prepare("SELECT project, COALESCE(sort_order, 0) FROM task_groups ORDER BY sort_order ASC, project ASC")
        .map_err(|e| (e500(), e.to_string()))?;
    let groups: Vec<(String, i64)> = group_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| (e500(), e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let mut col_stmt = conn
        .prepare("SELECT id, project, name, sort_order, created_at, updated_at FROM task_columns ORDER BY project, sort_order ASC")
        .map_err(|e| (e500(), e.to_string()))?;
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
        .map_err(|e| (e500(), e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let mut task_stmt = conn
        .prepare("SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks ORDER BY position ASC")
        .map_err(|e| (e500(), e.to_string()))?;
    let all_tasks: Vec<Task> = task_stmt
        .query_map([], |row| {
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
        })
        .map_err(|e| (e500(), e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(
        groups
            .into_iter()
            .map(|(project, sort_order)| GroupData {
                columns: columns
                    .iter()
                    .filter(|c| c.project == project)
                    .map(|col| ColumnWithTasks {
                        tasks: all_tasks
                            .iter()
                            .filter(|t| t.column_id == col.id)
                            .cloned()
                            .collect(),
                        column: col.clone(),
                    })
                    .collect(),
                project,
                sort_order,
            })
            .collect(),
    ))
}

async fn add_task(
    State(state): State<SharedState>,
    Json(req): Json<AddTaskRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let id = Uuid::new_v4().to_string();
    let project = req.project.unwrap_or_else(|| "default".to_string());
    let now = chrono::Utc::now().to_rfc3339();

    let conn = state::lock_db(&state.db)?;

    conn.execute(
        "INSERT OR IGNORE INTO task_groups (project, sort_order) VALUES (?1, 0)",
        rusqlite::params![project],
    )
    .map_err(|e| (e500(), e.to_string()))?;

    let column_id = match req.column_id {
        Some(cid) => cid,
        None => conn
            .query_row(
                "SELECT id FROM task_columns WHERE project = ?1 ORDER BY sort_order ASC LIMIT 1",
                rusqlite::params![project],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| {
                let cid = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO task_columns (id, project, name, sort_order) VALUES (?1, ?2, '任务', 0)",
                    rusqlite::params![cid, project],
                ).ok();
                cid
            }),
    };

    let max_pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM tasks WHERE column_id = ?1",
            rusqlite::params![column_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO tasks (id, project, title, progress, column_id, position, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, project, req.title, column_id, max_pos, now, now],
    )
    .map_err(|e| (e500(), e.to_string()))?;

    Ok(Json(Task {
        id,
        project,
        title: req.title,
        done: false,
        progress: 0,
        column_id,
        position: max_pos,
        created_at: now.clone(),
        updated_at: now,
    }))
}

async fn update_task(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<Task>, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;

    if let Some(title) = &req.title {
        conn.execute(
            "UPDATE tasks SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![title, id],
        )
        .map_err(|e| (e500(), e.to_string()))?;
    }
    if let Some(done) = req.done {
        let done_int: i32 = if done { 1 } else { 0 };
        let progress: i32 = if done { 100 } else { 0 };
        conn.execute(
            "UPDATE tasks SET done = ?1, progress = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![done_int, progress, id],
        )
        .map_err(|e| (e500(), e.to_string()))?;
    }
    if let Some(progress) = req.progress {
        let progress = progress.clamp(0, 100);
        let done = if progress >= 100 { 1 } else { 0 };
        conn.execute(
            "UPDATE tasks SET progress = ?1, done = ?2, updated_at = datetime('now') WHERE id = ?3",
            rusqlite::params![progress, done, id],
        )
        .map_err(|e| (e500(), e.to_string()))?;
    }

    conn.query_row(
        "SELECT id, project, title, done, COALESCE(progress,0), COALESCE(column_id,''), COALESCE(position,0), created_at, updated_at FROM tasks WHERE id = ?1",
        rusqlite::params![id],
        |row| {
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
        },
    )
    .map(Json)
    .map_err(|_| (StatusCode::NOT_FOUND, "Task not found".to_string()))
}

async fn delete_task_handler(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;
    let affected = conn
        .execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| (e500(), e.to_string()))?;
    if affected == 0 {
        return Err((StatusCode::NOT_FOUND, "Task not found".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn e500() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}
