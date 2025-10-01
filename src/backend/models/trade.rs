use bson::{DateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct TradeData {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: String,
    pub ts: String,
    pub profit_sol: f64,
    pub fees_lamports: i64,
    pub fees_sol: f64,
    pub roi_pct: f64,
    pub program_runtime_ms: i64,
    pub created_at: DateTime,
}

impl TradeData {
    pub fn new(
        user_id: String,
        profit_sol: f64,
        fees_lamports: i64,
        fees_sol: f64,
        roi_pct: f64,
        program_runtime_ms: i64,
    ) -> Self {
        Self {
            id: None,
            user_id,
            ts: chrono::Utc::now().to_rfc3339(),
            profit_sol,
            fees_lamports,
            fees_sol,
            roi_pct,
            program_runtime_ms,
            created_at: DateTime::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TradeDataResponse {
    pub id: String,
    pub user_id: String,
    pub ts: String,
    pub profit_sol: f64,
    pub fees_lamports: i64,
    pub fees_sol: f64,
    pub roi_pct: f64,
    pub program_runtime_ms: i64,
    pub created_at: DateTime,
}

impl From<TradeData> for TradeDataResponse {
    fn from(trade: TradeData) -> Self {
        Self {
            id: trade.id.unwrap_or_default().to_hex(),
            user_id: trade.user_id,
            ts: trade.ts,
            profit_sol: trade.profit_sol,
            fees_lamports: trade.fees_lamports,
            fees_sol: trade.fees_sol,
            roi_pct: trade.roi_pct,
            program_runtime_ms: trade.program_runtime_ms,
            created_at: trade.created_at,
        }
    }
}
