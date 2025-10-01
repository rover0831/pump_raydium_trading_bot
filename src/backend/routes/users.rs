use axum::{routing::get, Router};
use crate::backend::{
    db::connection::AppDatabase,
    handlers::users::get_current_user,
};

pub fn user_routes() -> Router<AppDatabase> {
    Router::new()
        .route("/me", get(get_current_user))
}
