use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use nalu_shared::models::Schedule;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::{self, SharedState};

#[derive(Debug, Deserialize)]
pub struct AddScheduleRequest {
    pub title: String,
    pub scheduled_at: String,
    pub reminder_minutes: Option<i32>,
}

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/schedules", get(get_schedules).post(add_schedule))
        .route("/schedules/{id}", put(toggle_schedule).delete(delete_schedule_handler))
}

async fn get_schedules(
    State(state): State<SharedState>,
) -> Result<Json<Vec<Schedule>>, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;
    let mut stmt = conn
        .prepare("SELECT id, title, scheduled_at, reminder_minutes, done, created_at FROM schedules ORDER BY scheduled_at ASC")
        .map_err(|e| (e500(), e.to_string()))?;
    let schedules: Vec<Schedule> = stmt
        .query_map([], |row| {
            Ok(Schedule {
                id: row.get(0)?,
                title: row.get(1)?,
                scheduled_at: row.get(2)?,
                reminder_minutes: row.get(3)?,
                done: row.get::<_, i32>(4)? != 0,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| (e500(), e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(Json(schedules))
}

async fn add_schedule(
    State(state): State<SharedState>,
    Json(req): Json<AddScheduleRequest>,
) -> Result<Json<Schedule>, (StatusCode, String)> {
    let id = Uuid::new_v4().to_string();
    let reminder_minutes = req.reminder_minutes.unwrap_or(5);
    let now = chrono::Utc::now().to_rfc3339();

    let conn = state::lock_db(&state.db)?;
    conn.execute(
        "INSERT INTO schedules (id, title, scheduled_at, reminder_minutes, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, req.title, req.scheduled_at, reminder_minutes, now],
    )
    .map_err(|e| (e500(), e.to_string()))?;

    Ok(Json(Schedule {
        id,
        title: req.title,
        scheduled_at: req.scheduled_at,
        reminder_minutes,
        done: false,
        created_at: now,
    }))
}

async fn toggle_schedule(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;
    conn.execute(
        "UPDATE schedules SET done = 1 - done WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| (e500(), e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_schedule_handler(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;
    let affected = conn
        .execute(
            "DELETE FROM schedules WHERE id = ?1",
            rusqlite::params![id],
        )
        .map_err(|e| (e500(), e.to_string()))?;
    if affected == 0 {
        return Err((StatusCode::NOT_FOUND, "Schedule not found".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn e500() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}
