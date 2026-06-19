use std::sync::Arc;

use crate::config::Config;
use crate::db::{self, DbConn};

pub struct AppState {
    pub db: DbConn,
    pub config: Config,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub async fn new(config: &Config) -> Result<SharedState, String> {
        let db = db::open(&config.db_path())?;

        Ok(Arc::new(Self {
            db,
            config: Config {
                port: config.port,
                data_dir: config.data_dir.clone(),
                pairing_code: config.pairing_code.clone(),
                jwt_secret: config.jwt_secret.clone(),
                jwt_expiry_days: config.jwt_expiry_days,
            },
        }))
    }
}

use axum::http::StatusCode;
use rusqlite::Connection;
use std::sync::MutexGuard;

/// Helper: lock the DB mutex and convert PoisonError to a (StatusCode, String) error.
pub fn lock_db(db: &DbConn) -> Result<MutexGuard<'_, Connection>, (StatusCode, String)> {
    db.lock()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
