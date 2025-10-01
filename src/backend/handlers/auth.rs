use axum::{extract::State, http::HeaderMap, response::Json};
use validator::Validate;

use crate::backend::{
    db::connection::AppDatabase,
    error::{AppError, AppResult},
    models::auth::{AuthResponse, SigninRequest, SignupRequest},
    models::user::UserResponse,
    services::user_service::UserService,
    services::auth_service::AuthService,
};

pub async fn signup(
    State(database): State<AppDatabase>,
    Json(payload): Json<SignupRequest>,
) -> AppResult<Json<AuthResponse>> {
    // Validate request
    payload
        .validate()
        .map_err(|e| AppError::validation(format!("Validation failed: {:?}", e)))?;

    let auth_service = AuthService::new(database);
    let response = auth_service.signup(payload).await?;

    Ok(Json(response))
}

pub async fn signin(
    State(database): State<AppDatabase>,
    Json(payload): Json<SigninRequest>,
) -> AppResult<Json<AuthResponse>> {
    // Validate request
    payload
        .validate()
        .map_err(|e| AppError::validation(format!("Validation failed: {:?}", e)))?;

    // Use real authentication service
    let auth_service = AuthService::new(database);
    
    let response = match auth_service.signin(payload).await {
        Ok(resp) => {
            resp
        }
        Err(e) => {
            return Err(e);
        }
    };

    Ok(Json(response))
}

pub async fn get_current_user(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
) -> AppResult<Json<UserResponse>> {
    let user_service = UserService::new(database);
    let user = user_service.get_current_user(headers).await?;
    Ok(Json(user))
}