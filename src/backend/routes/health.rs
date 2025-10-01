use axum::{response::Json, routing::get, Router};
use serde_json::json;

pub fn health_routes() -> Router {
    Router::new().route("/", get(health_check))
}

pub async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "message": "User Authentication API is running",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}
