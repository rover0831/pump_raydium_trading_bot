use std::sync::Arc;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use tokio::sync::RwLock;

pub static USER_LIST: Lazy<Arc<RwLock<Vec<crate::backend::services::bot_service::UserBotData>>>> =
    Lazy::new(|| Arc::new(RwLock::new(vec![])));

#[allow(dead_code)]
pub static REAL_POOL_INFO: Lazy<Arc<RwLock<HashMap<String, Vec<crate::backend::services::bot_service::RealPoolInfo>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

    