use axum::{
    routing::{get},
    Router,
};
use tower_http::cors::{CorsLayer, Any};
use std::env;

use crate::backend::{
    db::connection::AppDatabase,
    routes::{auth, bot, health, users, trade},
};

pub fn create_app(database: AppDatabase) -> Router {
    // Create CORS layer with proper configuration for production
    let cors = if env::var("RUST_ENV").unwrap_or_default() == "production" {
        // Production CORS - more restrictive
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
            .allow_credentials(true)
    } else {
        // Development CORS - permissive
        CorsLayer::permissive()
    };

    // Build application with routes
    Router::new()
        .route("/", get(health::health_check))
        .nest("/auth", auth::auth_routes())
        .nest("/users", users::user_routes())
        .nest("/bots", bot::bot_routes())
        .nest("/trades", trade::trade_routes())
        .with_state(database)
        .layer(cors)
}
