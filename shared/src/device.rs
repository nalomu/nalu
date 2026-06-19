use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRequest {
    pub pairing_code: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingResponse {
    pub device_id: String,
    pub token: String,
}

/// JWT claims embedded in auth tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceClaims {
    pub sub: String, // device_id
    pub device_name: String,
    pub iat: usize,
    pub exp: usize,
}
