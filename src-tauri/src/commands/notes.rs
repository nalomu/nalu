use crate::db::database::get_connection;
use crate::sync::changelog;
use nalu_shared::models::Note;
use nalu_shared::sync_protocol::{OP_DELETE, OP_INSERT, OP_UPDATE};

#[tauri::command]
pub fn get_notes(note_type: Option<String>) -> Result<Vec<Note>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    let notes = match &note_type {
        Some(t) => {
            let mut stmt = conn
                .prepare("SELECT id, title, content, tags, note_type, created_at, updated_at FROM notes WHERE note_type = ?1 ORDER BY updated_at DESC")
                .map_err(|e| e.to_string())?;
            let rows: Vec<Note> = stmt
                .query_map(rusqlite::params![t], |row| {
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
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            rows
        }
        None => {
            let mut stmt = conn
                .prepare("SELECT id, title, content, tags, note_type, created_at, updated_at FROM notes ORDER BY updated_at DESC")
                .map_err(|e| e.to_string())?;
            let rows: Vec<Note> = stmt
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
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            rows
        }
    };

    Ok(notes)
}

#[tauri::command]
pub fn add_note(
    title: String,
    content: Option<String>,
    tags: Option<String>,
    note_type: Option<String>,
) -> Result<Note, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let content = content.unwrap_or_default();
    let tags = tags.unwrap_or_default();
    let note_type = note_type.unwrap_or_else(|| "memo".to_string());

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "INSERT INTO notes (id, title, content, tags, note_type) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, title, content, tags, note_type],
    )
    .map_err(|e| e.to_string())?;

    let note = Note {
        id,
        title,
        content,
        tags,
        note_type,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    changelog::record_change(
        conn,
        "notes",
        &note.id,
        OP_INSERT,
        &serde_json::to_string(&note).unwrap_or_default(),
    )?;

    Ok(note)
}

#[tauri::command]
pub fn update_note(
    id: String,
    title: Option<String>,
    content: Option<String>,
    tags: Option<String>,
) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();

    if let Some(t) = title {
        conn.execute(
            "UPDATE notes SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![t, id],
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(c) = content {
        conn.execute(
            "UPDATE notes SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![c, id],
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(t) = tags {
        conn.execute(
            "UPDATE notes SET tags = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![t, id],
        )
        .map_err(|e| e.to_string())?;
    }

    // Record changelog with the full updated note
    let note: Note = conn.query_row(
        "SELECT id, title, content, tags, note_type, created_at, updated_at FROM notes WHERE id = ?1",
        rusqlite::params![id],
        |row| Ok(Note {
            id: row.get(0)?,
            title: row.get(1)?,
            content: row.get(2)?,
            tags: row.get(3)?,
            note_type: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        }),
    ).map_err(|e| e.to_string())?;

    changelog::record_change(
        conn,
        "notes",
        &note.id,
        OP_UPDATE,
        &serde_json::to_string(&note).unwrap_or_default(),
    )?;

    Ok(())
}

#[tauri::command]
pub fn delete_note(id: String) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute("DELETE FROM notes WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;

    changelog::record_change(conn, "notes", &id, OP_DELETE, "{}")?;
    Ok(())
}
