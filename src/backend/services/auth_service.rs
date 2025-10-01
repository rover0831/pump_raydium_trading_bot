use crate::backend::{
    auth::jwt_service::JwtService,
    db::connection::AppDatabase,
    db::user_repository::UserRepository,
    db::bot_repository::BotRepository,
    services::bot_service::BotService,
    error::{AppError, AppResult},
    models::{
        auth::{AuthResponse, SigninRequest, SignupRequest},
        user::User,
        bot::BotSettings,
    },
};
use tracing::{info};

pub struct AuthService {
    user_repo: UserRepository,
    bot_repo: BotRepository,
    jwt_service: JwtService,
    bot_service: BotService,
}

impl AuthService {
    pub fn new(database: AppDatabase) -> Self {
        Self {
            user_repo: UserRepository::new(database.clone()),
            bot_repo: BotRepository::new(database.clone()),
            jwt_service: JwtService::new(),
            bot_service: BotService::new(database.clone()),
        }
    }

    /// Register a new user (signup)
    pub async fn signup(&self, request: SignupRequest) -> AppResult<AuthResponse> {
        // Check if user already exists
        if self
            .user_repo
            .find_by_email(&request.email)
            .await?
            .is_some()
        {
            return Err(AppError::conflict("Email already in use"));
        }

        if self
            .user_repo
            .find_by_username(&request.username)
            .await?
            .is_some()
        {
            return Err(AppError::conflict("Username already taken"));
        }

        // Automatically create initial bot settings for the new user
        let (private_key, public_key) = User::get_or_create_user_private_key().await?;

        // Create new user with hashed password
        let user = User::new(
            request.email.clone(),
            request.username.clone(),
            request.password.clone(),
            private_key.to_string(),
            public_key.to_string(),
        )
        .map_err(|e| AppError::internal(format!("Failed to create user: {}", e)))?;

        let created_user = self.user_repo.create(user).await?;

        println!("✅ Created user: {:?}", created_user);

        // Extract user ID (must exist after creation)
        let user_id = created_user
            .id
            .as_ref()
            .map(|id| id.to_hex())
            .ok_or_else(|| AppError::internal("User ID missing after creation"))?;

        // Create initial bot settings for the new user
        let initial_bot = BotSettings::create_default_bot(user_id.clone());

        let created_bot = self.bot_repo.create(initial_bot.clone()).await?;

        info!("✅ Initial bot created for user: {} - Bot ID: {}", user_id, created_bot.id.unwrap().to_hex());

        // Generate JWT token
        let token = self
            .jwt_service
            .create_token(&user_id)
            .map_err(|e| AppError::internal(format!("Failed to create JWT: {}", e)))?;

        info!(
            "✅ New user signed up: {} ({}) - Public Key: {}",
            created_user.username, created_user.email, created_user.public_key
        );

        Ok(AuthResponse {
            token,
            user: created_user.into(),
            bot: created_bot.into(),
        })
    }

    /// Authenticate existing user (signin)
    pub async fn signin(&self, request: SigninRequest) -> AppResult<AuthResponse> {
        // Find user by email
        let user = self
            .user_repo
            .find_by_email(&request.email)
            .await?
            .ok_or_else(|| AppError::auth("Invalid email or password"))?;

        // Verify password
        if !user.verify_password(&request.password).unwrap_or(false) {
            return Err(AppError::auth("Invalid email or password"));
        }

        // Extract user ID
        let user_id = user
            .id
            .as_ref()
            .map(|id| id.to_hex())
            .ok_or_else(|| AppError::internal("User ID missing in DB"))?;

        let bots = self.bot_service.get_user_bots(&user_id).await?;

        // Generate JWT token
        let token = self
            .jwt_service
            .create_token(&user_id)
            .map_err(|e| AppError::internal(format!("Failed to create JWT: {}", e)))?;

        info!("✅ User signed in: {}", user.email);

        Ok(AuthResponse {
            token,
            user: user.into(),
            bot: bots.first().cloned().unwrap_or_default(),
        })
    }
}
