use std::path::PathBuf;

pub struct Config {
    pub port: u16,
    pub data_dir: PathBuf,
    pub pairing_code: String,
    pub jwt_secret: String,
    pub jwt_expiry_days: i64,
}

impl Config {
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("nalu-server.db")
    }
}

pub fn load() -> Config {
    let port: u16 = std::env::var("NALU_SERVER_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);

    let data_dir = std::env::var("NALU_SERVER_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data"));

    std::fs::create_dir_all(&data_dir).ok();

    let pairing_code = std::env::var("NALU_SERVER_PAIRING_CODE")
        .unwrap_or_else(|_| {
            // Generate a random 6-digit code on first run if not set
            let code_path = data_dir.join("pairing_code.txt");
            if code_path.exists() {
                std::fs::read_to_string(&code_path).unwrap_or_else(|_| generate_code())
            } else {
                let code = generate_code();
                let _ = std::fs::write(&code_path, &code);
                code
            }
        })
        .trim()
        .to_string();

    let jwt_secret = std::env::var("NALU_SERVER_JWT_SECRET")
        .unwrap_or_else(|_| {
            let secret_path = data_dir.join("jwt_secret.txt");
            if secret_path.exists() {
                std::fs::read_to_string(&secret_path).unwrap_or_else(|_| generate_secret())
            } else {
                let secret = generate_secret();
                let _ = std::fs::write(&secret_path, &secret);
                secret
            }
        })
        .trim()
        .to_string();

    Config {
        port,
        data_dir,
        pairing_code,
        jwt_secret,
        jwt_expiry_days: 365,
    }
}

fn generate_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.r#gen_range(0..1_000_000))
}

fn generate_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex_encode(&bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
