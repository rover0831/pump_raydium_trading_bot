use axum::http::HeaderMap;
use crate::backend::{
    db::connection::AppDatabase,
    error::{AppError, AppResult},
    models::user::UserResponse,
    db::user_repository::UserRepository,
    auth::jwt_service::JwtService,
};

pub struct UserService {
    user_repo: UserRepository,
    jwt_service: JwtService,
}

impl UserService {
    pub fn new(database: AppDatabase) -> Self {
        Self {
            user_repo: UserRepository::new(database),
            jwt_service: JwtService::new(),
        }
    }

    pub async fn get_current_user(&self, headers: HeaderMap) -> AppResult<UserResponse> {
        // Extract token from Authorization header
        let auth_header = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| AppError::auth("Authorization header is required"))?;

        let token = crate::backend::auth::jwt_service::extract_token_from_header(auth_header)
            .ok_or_else(|| AppError::auth("Token must be in format: Bearer <token>"))?;

        // Verify JWT token
        let claims = self.jwt_service.verify_token(token)?;

        // Find user by ID from token
        let user = self.user_repo
            .find_by_id(&claims.sub)
            .await?
            .ok_or_else(|| AppError::not_found("User associated with token not found"))?;

        Ok(user.into())
    }
}
