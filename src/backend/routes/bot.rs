use axum::{routing::put, routing::get, Router};

use crate::backend::{
    db::connection::AppDatabase,
    handlers::bot::{get_user_bots, start_bot, stop_bot, update_mev_config, update_trading_params},
};

pub fn bot_routes() -> Router<AppDatabase> {
    Router::new()
        .route("/", get(get_user_bots))
        .route("/trading", put(update_trading_params))
        .route("/mev", put(update_mev_config))
        .route("/start", get(start_bot))
        .route("/stop", get(stop_bot))
}
