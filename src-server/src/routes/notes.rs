use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
};
use nalu_shared::models::Note;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::{self, SharedState};

#[derive(Debug, Deserialize)]
pub struct AddNoteRequest {
    pub title: String,
    pub content: Option<String>,
    pub tags: Option<String>,
    pub note_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<String>,
}

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/notes", get(get_notes).post(add_note))
        .route("/notes/{id}", put(update_note).delete(delete_note_handler))
}

async fn get_notes(
    State(state): State<SharedState>,
) -> Result<Json<Vec<Note>>, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;
    let mut stmt = conn
        .prepare("SELECT id, title, content, tags, note_type, created_at, updated_at FROM notes ORDER BY updated_at DESC")
        .map_err(|e| (e500(), e.to_string()))?;
    let notes: Vec<Note> = stmt
        .query_map([], |row| {
            Ok(Note {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                tags: row.get(3)?,
                note_type: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|e| (e500(), e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(Json(notes))
}

async fn add_note(
    State(state): State<SharedState>,
    Json(req): Json<AddNoteRequest>,
) -> Result<Json<Note>, (StatusCode, String)> {
    let id = Uuid::new_v4().to_string();
    let content = req.content.unwrap_or_default();
    let tags = req.tags.unwrap_or_default();
    let note_type = req.note_type.unwrap_or_else(|| "memo".to_string());
    let now = chrono::Utc::now().to_rfc3339();

    let conn = state::lock_db(&state.db)?;
    conn.execute(
        "INSERT INTO notes (id, title, content, tags, note_type, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, req.title, content, tags, note_type, now, now],
    )
    .map_err(|e| (e500(), e.to_string()))?;

    Ok(Json(Note {
        id,
        title: req.title,
        content,
        tags,
        note_type,
        created_at: now.clone(),
        updated_at: now,
    }))
}

async fn update_note(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;

    let mut updated = false;
    if let Some(title) = &req.title {
        conn.execute(
            "UPDATE notes SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![title, id],
        )
        .map_err(|e| (e500(), e.to_string()))?;
        updated = true;
    }
    if let Some(content) = &req.content {
        conn.execute(
            "UPDATE notes SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![content, id],
        )
        .map_err(|e| (e500(), e.to_string()))?;
        updated = true;
    }
    if let Some(tags) = &req.tags {
        conn.execute(
            "UPDATE notes SET tags = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![tags, id],
        )
        .map_err(|e| (e500(), e.to_string()))?;
        updated = true;
    }

    if !updated {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_note_handler(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let conn = state::lock_db(&state.db)?;
    let affected = conn
        .execute("DELETE FROM notes WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| (e500(), e.to_string()))?;
    if affected == 0 {
        return Err((StatusCode::NOT_FOUND, "Note not found".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn e500() -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}
