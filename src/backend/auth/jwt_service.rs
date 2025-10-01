use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend::error::{AppResult, AppError};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // User ID
    pub exp: usize,  // Expiration time
    pub iat: usize,  // Issued at
}

pub struct JwtService {
    secret: Vec<u8>,
}

impl JwtService {
    pub fn new() -> Self {
        // In a real app, you'd get this from config
        let secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());
        
        Self {
            secret: secret.into_bytes(),
        }
    }

    pub fn create_token(&self, user_id: &str) -> AppResult<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;
        
        let expiration = now + (24 * 60 * 60); // 24 hours
        
        let claims = Claims {
            sub: user_id.to_string(),
            exp: expiration,
            iat: now,
        };
        
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.secret),
        ).map_err(|e| AppError::internal(format!("Failed to create token: {:?}", e)))?;
        
        Ok(token)
    }

    pub fn verify_token(&self, token: &str) -> AppResult<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.secret),
            &Validation::default(),
        ).map_err(|e| AppError::internal(format!("Failed to verify token: {:?}", e)))?;
        
        Ok(token_data.claims)
    }
}

pub fn extract_token_from_header(auth_header: &str) -> Option<&str> {
    if auth_header.starts_with("Bearer ") {
        Some(&auth_header[7..])
    } else {
        None
    }
}
