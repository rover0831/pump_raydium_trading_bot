use axum::{routing::post, routing::get, Router};
use crate::backend::{
    db::connection::AppDatabase,
    handlers::auth::{signup, signin, get_current_user},
};

pub fn auth_routes() -> Router<AppDatabase> {
    Router::new()
    .route("/signup", post(signup))
    .route("/signin", post(signin))
    .route("/me", get(get_current_user))
}
