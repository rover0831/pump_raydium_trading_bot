use axum::{
    extract::State,
    http::HeaderMap,
    response::Json,
};

use crate::backend::{
    db::connection::AppDatabase,
    error::AppResult,
    models::user::UserResponse,
    services::user_service::UserService,
};

pub async fn get_current_user(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
) -> AppResult<Json<UserResponse>> {
    let user_service = UserService::new(database);
    let user = user_service.get_current_user(headers).await?;
    
    Ok(Json(user))
}
