use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;

use crate::backend::{
    db::connection::AppDatabase,
    services::trade_service::TradeService,
};

#[derive(Debug, Deserialize)]
pub struct TradeQuery {
    pub limit: Option<i64>,
}

pub async fn get_trade_data(
    State(database): State<AppDatabase>,
    Query(query): Query<TradeQuery>,
) -> Result<Json<Vec<crate::backend::models::trade::TradeDataResponse>>, StatusCode> {
    let trade_service = TradeService::new(database);
    let limit = query.limit.unwrap_or(50);

    println!("Getting trade data for limit: {}", limit);
    match trade_service.get_recent_trades(limit).await {
        Ok(trades) => Ok(Json(trades)),
        Err(e) => {
            eprintln!("Failed to get trade data: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_user_trades(
    State(database): State<AppDatabase>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<crate::backend::models::trade::TradeDataResponse>>, StatusCode> {
    let trade_service = TradeService::new(database);
    
    // Basic validation
    if user_id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    match trade_service.get_user_trades(&user_id).await {
        Ok(mut trades) => {
            // Sort by created_at descending (most recent first) and limit results
            trades.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            Ok(Json(trades))
        },
        Err(e) => {
            eprintln!("Failed to get user trades: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
