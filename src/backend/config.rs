use std::env;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub mongodb_uri: String,
    pub jwt_secret: String,
    pub rust_log: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenv::dotenv();

        Ok(Self {
            mongodb_uri: env::var("MONGODB_URI")
                .unwrap_or_else(|_| "mongodb://localhost:27017".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string()),
            rust_log: env::var("RUST_LOG")
                .unwrap_or_else(|_| "info".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .unwrap_or(3000)
        })
    }

    pub fn mongodb_connection_string(&self) -> String {
        self.mongodb_uri.clone()
    }

    pub fn jwt_secret_bytes(&self) -> &[u8] {
        self.jwt_secret.as_bytes()
    }
}
