use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{Json, Response},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use nalu_shared::device::{DeviceClaims, PairingRequest, PairingResponse};
use serde_json::json;
use uuid::Uuid;

use crate::state::{self, SharedState};

/// POST /api/auth/pair
pub async fn pair_handler(
    State(state): State<SharedState>,
    Json(req): Json<PairingRequest>,
) -> Result<Json<PairingResponse>, (StatusCode, String)> {
    if req.pairing_code != state.config.pairing_code {
        return Err((StatusCode::FORBIDDEN, "Invalid pairing code".to_string()));
    }

    let device_id = Uuid::new_v4().to_string();
    let device_name = req.device_name.clone();

    // Store device
    {
        let conn = state::lock_db(&state.db)?;
        conn.execute(
            "INSERT OR REPLACE INTO devices (device_id, device_name, last_seen_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![device_id, device_name],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    let now = chrono::Utc::now().timestamp() as usize;
    let claims = DeviceClaims {
        sub: device_id.clone(),
        device_name,
        iat: now,
        exp: now + (state.config.jwt_expiry_days as usize * 86400),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(PairingResponse { device_id, token }))
}

/// Axum middleware: verify JWT and inject device_id into request extensions.
pub async fn auth_middleware(
    State(state): State<SharedState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(t) => t,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Missing Authorization header"})),
            ));
        }
    };

    let token_data = decode::<DeviceClaims>(
        token,
        &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": format!("Invalid token: {}", e)})),
        )
    })?;

    request.extensions_mut().insert(token_data.claims);
    Ok(next.run(request).await)
}
