use crate::sync::client::{self, SyncConfig, SyncResult};
use tauri::{AppHandle, Manager};

/// Pair with the sync server.
#[tauri::command]
pub async fn sync_pair(
    app: AppHandle,
    server_url: String,
    pairing_code: String,
    device_name: String,
) -> Result<SyncConfig, String> {
    let config = client::pair(&server_url, &pairing_code, &device_name).await?;
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    config.save(&app_dir)?;
    Ok(config)
}

/// Run a full sync cycle: push pending changes, pull remote changes.
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<SyncResult, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let config = SyncConfig::load(&app_dir)
        .ok_or_else(|| "Sync not configured".to_string())?;
    client::sync(&config).await
}

/// Get current sync configuration (without the token).
#[tauri::command]
pub fn sync_get_config(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let config = SyncConfig::load(&app_dir);
    Ok(config.map(|c| {
        serde_json::json!({
            "server_url": c.server_url,
            "device_id": c.device_id,
            "device_name": c.device_name,
        })
    }))
}

/// Disconnect: remove sync config.
#[tauri::command]
pub fn sync_disconnect(app: AppHandle) -> Result<(), String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    SyncConfig::remove(&app_dir);
    Ok(())
}
