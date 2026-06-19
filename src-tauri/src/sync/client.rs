use nalu_shared::{
    device::PairingRequest,
    sync_protocol::{SYNC_TABLES, SyncAck, SyncPullRequest, SyncPushRequest},
};

use super::changelog;

/// Sync configuration stored in the app data dir.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncConfig {
    pub server_url: String,
    pub device_id: String,
    pub device_name: String,
    pub auth_token: String,
}

impl SyncConfig {
    pub fn load(app_dir: &std::path::Path) -> Option<Self> {
        let path = app_dir.join("sync_config.json");
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(&self, app_dir: &std::path::Path) -> Result<(), String> {
        let path = app_dir.join("sync_config.json");
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, data).map_err(|e| e.to_string())
    }

    pub fn remove(app_dir: &std::path::Path) {
        let _ = std::fs::remove_file(app_dir.join("sync_config.json"));
    }
}

/// Pair with the server: send pairing code, get back JWT token and device_id.
pub async fn pair(
    server_url: &str,
    pairing_code: &str,
    device_name: &str,
) -> Result<SyncConfig, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "{}/api/auth/pair",
            server_url.trim_end_matches('/')
        ))
        .json(&PairingRequest {
            pairing_code: pairing_code.to_string(),
            device_name: device_name.to_string(),
        })
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Pairing failed: {}", body));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(SyncConfig {
        server_url: server_url.to_string(),
        device_id: body["device_id"].as_str().unwrap_or("").to_string(),
        device_name: device_name.to_string(),
        auth_token: body["token"].as_str().unwrap_or("").to_string(),
    })
}

/// Push pending changes to the server, then pull remote changes.
pub async fn sync(config: &SyncConfig) -> Result<SyncResult, String> {
    let client = reqwest::Client::new();
    let base = config.server_url.trim_end_matches('/');

    // 1. Push pending entries
    let entries = changelog::get_pending_entries()?;
    let last_server_ts = changelog::get_last_server_ts()?;
    let mut max_seen_server_ts = last_server_ts;

    let mut pushed_count = 0i64;
    let mut conflict_count = 0i64;

    if !entries.is_empty() {
        let push_resp = client
            .post(format!("{}/api/sync/push", base))
            .header("Authorization", format!("Bearer {}", config.auth_token))
            .json(&SyncPushRequest {
                device_id: config.device_id.clone(),
                last_server_ts,
                entries,
            })
            .send()
            .await
            .map_err(|e| format!("Push failed: {}", e))?;

        if !push_resp.status().is_success() {
            let status = push_resp.status();
            let body = push_resp.text().await.unwrap_or_default();
            return Err(format!("Push failed: {} {}", status, body));
        }

        let body: serde_json::Value = push_resp.json().await.map_err(|e| e.to_string())?;

        // Mark accepted entries as synced.
        if let Some(accepted) = body["accepted"].as_array() {
            let acks: Vec<SyncAck> = accepted
                .iter()
                .map(|a| SyncAck {
                    client_entry_id: a["client_entry_id"].as_i64().unwrap_or(0),
                    server_ts: a["server_ts"].as_i64().unwrap_or(0),
                })
                .collect();
            pushed_count = acks.len() as i64;
            changelog::mark_synced(&acks)?;
            if let Some(server_ts) = acks.iter().map(|ack| ack.server_ts).max() {
                max_seen_server_ts = max_seen_server_ts.max(server_ts);
            }
        }

        // Apply conflict entries (server versions) and retire the rejected local entries.
        if let Some(conflicts) = body["conflicts"].as_array() {
            conflict_count = conflicts.len() as i64;
            let mut conflict_acks = Vec::new();
            for entry in conflicts {
                let table_name = entry["table_name"].as_str().unwrap_or("");
                let row_id = entry["row_id"].as_str().unwrap_or("");
                let payload = entry["payload"].as_str().unwrap_or("");
                let operation = entry["operation"].as_str().unwrap_or("update");
                super::with_sync_disabled(|| {
                    if operation == "delete" {
                        apply_remote_delete(table_name, row_id)
                    } else {
                        apply_remote_payload(table_name, row_id, payload)
                    }
                })?;

                if let (Some(client_entry_id), Some(server_ts)) =
                    (entry["id"].as_i64(), entry["server_ts"].as_i64())
                {
                    max_seen_server_ts = max_seen_server_ts.max(server_ts);
                    conflict_acks.push(SyncAck {
                        client_entry_id,
                        server_ts,
                    });
                }
            }
            if !conflict_acks.is_empty() {
                changelog::mark_synced(&conflict_acks)?;
            }
        }
    }

    // 2. Pull remote changes
    let pull_resp = client
        .post(format!("{}/api/sync/pull", base))
        .header("Authorization", format!("Bearer {}", config.auth_token))
        .json(&SyncPullRequest {
            device_id: config.device_id.clone(),
            last_server_ts,
        })
        .send()
        .await
        .map_err(|e| format!("Pull failed: {}", e))?;

    let mut pulled_count = 0i64;
    if !pull_resp.status().is_success() {
        let status = pull_resp.status();
        let body = pull_resp.text().await.unwrap_or_default();
        return Err(format!("Pull failed: {} {}", status, body));
    }

    let body: serde_json::Value = pull_resp.json().await.map_err(|e| e.to_string())?;
    if let Some(entries) = body["entries"].as_array() {
        pulled_count = entries.len() as i64;
        for entry in entries {
            let table_name = entry["table_name"].as_str().unwrap_or("");
            let row_id = entry["row_id"].as_str().unwrap_or("");
            let payload = entry["payload"].as_str().unwrap_or("");
            let operation = entry["operation"].as_str().unwrap_or("update");

            super::with_sync_disabled(|| {
                if operation == "delete" {
                    apply_remote_delete(table_name, row_id)
                } else {
                    apply_remote_payload(table_name, row_id, payload)
                }
            })?;
        }
    }
    if let Some(latest_server_ts) = body["latest_server_ts"].as_i64() {
        max_seen_server_ts = max_seen_server_ts.max(latest_server_ts);
    }
    changelog::set_last_server_ts(max_seen_server_ts)?;

    Ok(SyncResult {
        pushed_count,
        pulled_count,
        conflict_count,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncResult {
    pub pushed_count: i64,
    pub pulled_count: i64,
    pub conflict_count: i64,
}

/// Apply a remote upsert payload to the local DB.
fn apply_remote_payload(table_name: &str, row_id: &str, payload: &str) -> Result<(), String> {
    let key_column = sync_key_column(table_name)?;
    let db = crate::db::database::get_connection()?;
    let conn = db.as_ref().unwrap();
    let fields: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(payload).map_err(|e| e.to_string())?;

    if fields.is_empty() {
        return Ok(());
    }

    // Check if row exists
    let exists: bool = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE {} = ?1",
                table_name, key_column
            ),
            rusqlite::params![row_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if exists {
        // UPDATE
        let mut sets = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for (key, value) in &fields {
            validate_identifier(key)?;
            if key == key_column {
                continue;
            }
            sets.push(format!("{} = ?", key));
            params.push(value_to_sql(value));
        }
        if !sets.is_empty() {
            let sets_str = sets.join(", ");
            let sql = format!(
                "UPDATE {} SET {} WHERE {} = ?",
                table_name, sets_str, key_column
            );
            params.push(Box::new(row_id.to_string()));
            let refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|v| v.as_ref()).collect();
            conn.execute(&sql, rusqlite::params_from_iter(refs.iter()))
                .map_err(|e| e.to_string())?;
        }
    } else {
        // INSERT
        let mut cols = Vec::new();
        let mut placeholders = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for (key, value) in &fields {
            validate_identifier(key)?;
            cols.push(key.clone());
            placeholders.push("?".to_string());
            params.push(value_to_sql(value));
        }
        if !cols.is_empty() {
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table_name,
                cols.join(", "),
                placeholders.join(", ")
            );
            let refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|v| v.as_ref()).collect();
            conn.execute(&sql, rusqlite::params_from_iter(refs.iter()))
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn apply_remote_delete(table_name: &str, row_id: &str) -> Result<(), String> {
    let key_column = sync_key_column(table_name)?;
    let db = crate::db::database::get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        &format!("DELETE FROM {} WHERE {} = ?1", table_name, key_column),
        rusqlite::params![row_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn sync_key_column(table_name: &str) -> Result<&'static str, String> {
    if !SYNC_TABLES.contains(&table_name) {
        return Err(format!("Unsupported sync table: {}", table_name));
    }
    Ok(match table_name {
        "task_groups" => "project",
        _ => "id",
    })
}

fn validate_identifier(identifier: &str) -> Result<(), String> {
    let valid = !identifier.is_empty()
        && identifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid sync payload field: {}", identifier))
    }
}

fn value_to_sql(v: &serde_json::Value) -> Box<dyn rusqlite::types::ToSql> {
    match v {
        serde_json::Value::String(s) => Box::new(s.clone()),
        serde_json::Value::Number(n) if n.is_i64() => Box::new(n.as_i64().unwrap()),
        serde_json::Value::Bool(b) => Box::new(*b as i32),
        _ => Box::new(v.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_key_column_uses_project_for_task_groups() {
        assert_eq!(sync_key_column("task_groups").unwrap(), "project");
        assert_eq!(sync_key_column("tasks").unwrap(), "id");
        assert!(sync_key_column("clipboard_history").is_err());
    }

    #[test]
    fn validate_identifier_rejects_sql_fragments() {
        assert!(validate_identifier("sort_order").is_ok());
        assert!(validate_identifier("name = 'x' --").is_err());
        assert!(validate_identifier("").is_err());
    }
}
