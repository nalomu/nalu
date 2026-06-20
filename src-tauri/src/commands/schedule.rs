use crate::db::database::get_connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub title: String,
    pub scheduled_at: String,
    pub reminder_minutes: i32,
    pub done: bool,
    pub created_at: String,
}

#[tauri::command]
pub fn get_schedules() -> Result<Vec<Schedule>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, title, scheduled_at, reminder_minutes, done, created_at FROM schedules ORDER BY scheduled_at ASC")
        .map_err(|e| e.to_string())?;
    let schedules = stmt
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
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(schedules)
}

#[tauri::command]
pub fn add_schedule(
    title: String,
    scheduled_at: String,
    reminder_minutes: Option<i32>,
) -> Result<Schedule, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let reminder_minutes = reminder_minutes.unwrap_or(0);

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "INSERT INTO schedules (id, title, scheduled_at, reminder_minutes) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, title, scheduled_at, reminder_minutes],
    )
    .map_err(|e| e.to_string())?;

    Ok(Schedule {
        id,
        title,
        scheduled_at,
        reminder_minutes,
        done: false,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
pub fn toggle_schedule(id: String) -> Result<bool, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE schedules SET done = 1 - done WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    let done: bool = conn
        .query_row(
            "SELECT done FROM schedules WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, i32>(0),
        )
        .map_err(|e| e.to_string())?
        != 0;
    Ok(done)
}

#[tauri::command]
pub fn delete_schedule(id: String) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute("DELETE FROM schedules WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
