use anyhow::Result;
use bson::{DateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::{Keypair, Signer};

use crate::backend::auth::password_service::PasswordService;

#[derive(Debug, Serialize, Clone)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub private_key: String,
    pub public_key: String,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

impl<'de> Deserialize<'de> for User {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct UserHelper {
            #[serde(rename = "_id")]
            id: Option<ObjectId>,
            email: String,
            username: String,
            password_hash: String,
            private_key: String,
            public_key: String,
            created_at: Option<DateTime>,
            updated_at: Option<DateTime>,
        }

        let helper = UserHelper::deserialize(deserializer)?;
        Ok(User {
            id: helper.id,
            email: helper.email,
            username: helper.username,
            password_hash: helper.password_hash,
            private_key: helper.private_key,
            public_key: helper.public_key,
            created_at: helper.created_at.unwrap_or_else(DateTime::now),
            updated_at: helper.updated_at.unwrap_or_else(DateTime::now),
        })
    }
}

impl User {
    pub fn new(
        email: String,
        username: String,
        password: String,
        private_key: String,
        public_key: String,
    ) -> Result<Self> {
        let password_hash = PasswordService::hash_password(&password)?;

        Ok(Self {
            id: None,
            email,
            username,
            password_hash,
            private_key,
            public_key,
            created_at: DateTime::now(),
            updated_at: DateTime::now(),
        })
    }

    pub fn verify_password(&self, password: &str) -> Result<bool> {
        PasswordService::verify_password(password, &self.password_hash)
    }

    pub fn update_password(&mut self, new_password: String) -> Result<()> {
        self.password_hash = PasswordService::hash_password(&new_password)?;
        self.updated_at = DateTime::now();
        Ok(())
    }

    pub fn update_profile(&mut self, email: Option<String>, username: Option<String>) {
        if let Some(email) = email {
            self.email = email;
        }
        if let Some(username) = username {
            self.username = username;
        }
        self.updated_at = DateTime::now();
    }
    /// Generate or retrieve a private key for a user
    /// This function handles the private key management securely
    /// Returns (private_key, public_key) tuple
    pub async fn get_or_create_user_private_key() -> Result<(String, String)> {
        // Generate a new keypair for the user
        let keypair = Keypair::new();
        let private_key = bs58::encode(keypair.to_bytes()).into_string();
        let public_key = keypair.pubkey().to_string();

        println!(
            "Generated new keypair for user - Public Key: {}",
            public_key
        );

        Ok((private_key, public_key))
    }
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub username: String,
    pub public_key: String,
    pub created_at: DateTime,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.unwrap().to_hex(),
            email: user.email,
            username: user.username,
            public_key: user.public_key,
            created_at: user.created_at,
        }
    }
}
