use axum::{
    routing::get,
    Router,
};

use crate::backend::{
    db::connection::AppDatabase,
    handlers::trade::{get_trade_data, get_user_trades},
};

pub fn trade_routes() -> Router<AppDatabase> {
    Router::new()
        .route("/data", get(get_trade_data))
        .route("/user/:user_id", get(get_user_trades))
}
