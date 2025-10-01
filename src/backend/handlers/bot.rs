use axum::{
    extract::State,
    http::HeaderMap,
    response::Json,
};
use serde::Deserialize;
use validator::Validate;

use crate::backend::{
    auth::jwt_service::extract_token_from_header,
    auth::jwt_service::JwtService,
    db::connection::AppDatabase,
    error::{AppError, AppResult},
    models::bot::BotSettingsResponse,
    services::bot_service::BotService,
};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateBotRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(min = 32, max = 44))]
    pub pool_address: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateTradingParamsRequest {
    #[validate(length(min = 32, max = 44))]
    pub pool_address: Option<String>,
    #[validate(range(min = 0.0001, max = 1000.0))]
    pub buy_sol_amount: Option<f64>,
    #[validate(range(min = 0.0001, max = 100.0))]
    pub entry_percent: Option<f64>,
    #[validate(range(min = 0.1, max = 50.0))]
    pub entry_slippage: Option<f64>,
    #[validate(range(min = 0.1, max = 100.0))]
    pub exit_slippage: Option<f64>,
    #[validate(range(min = 0.0001, max = 100.0))]
    pub stop_loss: Option<f64>,
    #[validate(range(min = 0.0001, max = 1000.0))]
    pub take_profit: Option<f64>,
    #[validate(range(min = 0, max = 86400))]
    pub auto_exit: Option<u64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMevConfigRequest {
    #[validate(length(min = 1, max = 20))]
    pub confirm_service: Option<String>,
    #[validate(range(min = 1, max = 1000000))]
    pub cu: Option<u64>,
    #[validate(range(min = 0, max = 1000000))]
    pub priority_fee: Option<u64>,
    #[validate(range(min = 0.0, max = 100.0))]
    pub third_party_fee: Option<f64>,
}

/// Extract user ID from JWT token
async fn get_user_id_from_token(headers: &HeaderMap) -> AppResult<String> {
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::auth("Authorization header is required"))?;

    let token = extract_token_from_header(auth_header)
        .ok_or_else(|| AppError::auth("Token must be in format: Bearer <token>"))?;

    let jwt_service = JwtService::new();
    let claims = jwt_service.verify_token(token)?;

    Ok(claims.sub)
}

pub async fn create_bot(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
    Json(payload): Json<CreateBotRequest>,
) -> AppResult<Json<BotSettingsResponse>> {
    payload.validate()
        .map_err(|e: validator::ValidationErrors| AppError::validation(format!("Validation failed: {:?}", e)))?;

    let user_id = get_user_id_from_token(&headers).await?;

    let bot_service = BotService::new(database);
    let bot = bot_service.create_bot(
        user_id,
        payload.name,
        payload.pool_address,
    ).await?;

    Ok(Json(bot))
}

pub async fn get_user_bots(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<BotSettingsResponse>>> {
    let user_id = get_user_id_from_token(&headers).await?;

    let bot_service = BotService::new(database);
    let bots = bot_service.get_user_bots(&user_id).await?;

    Ok(Json(bots))
}

pub async fn update_trading_params(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
    Json(payload): Json<UpdateTradingParamsRequest>,
) -> AppResult<Json<BotSettingsResponse>> {
    payload.validate()
        .map_err(|e: validator::ValidationErrors| AppError::validation(format!("Validation failed: {:?}", e)))?;

    let user_id = get_user_id_from_token(&headers).await?;

    let bot_service = BotService::new(database);
    let bot = bot_service.update_trading_params(
        &user_id,
        payload.pool_address,
        payload.buy_sol_amount,
        payload.entry_percent,
        payload.entry_slippage,
        payload.exit_slippage,
        payload.stop_loss,
        payload.take_profit,
        payload.auto_exit,
    ).await?;

    Ok(Json(bot))
}

pub async fn update_mev_config(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
    Json(payload): Json<UpdateMevConfigRequest>,
) -> AppResult<Json<BotSettingsResponse>> {
    payload.validate()
        .map_err(|e| AppError::validation(format!("Validation failed: {:?}", e)))?;

    let user_id = get_user_id_from_token(&headers).await?;

    let bot_service = BotService::new(database);
    let bot = bot_service.update_mev_config(
        &user_id,
        payload.confirm_service,
        payload.cu,
        payload.priority_fee,
        payload.third_party_fee,
    ).await?;

    Ok(Json(bot))
}

pub async fn start_bot(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
) -> AppResult<Json<String>> {
    let user_id = get_user_id_from_token(&headers).await?;

    let bot_service = BotService::new(database);
    let bot = bot_service.start_bot(&user_id).await?;

    Ok(Json(bot))
}

pub async fn stop_bot(
    State(database): State<AppDatabase>,
    headers: HeaderMap,
) -> AppResult<Json<String>> {
    let user_id = get_user_id_from_token(&headers).await?;
    let bot_service = BotService::new(database);
    let bot = bot_service.stop_bot(&user_id).await?;
    Ok(Json(bot))
}