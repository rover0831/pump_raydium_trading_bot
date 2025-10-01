use anyhow::Result;
use crate::backend::{
    db::connection::AppDatabase,
    db::trade_repository::{TradeRepository},
    models::trade::{TradeData, TradeDataResponse},
};

pub struct TradeService {
    trade_repo: TradeRepository,
}

impl TradeService {
    pub fn new(database: AppDatabase) -> Self {
        Self {
            trade_repo: TradeRepository::new(database),
        }
    }

    pub async fn save_trade_data(
        &self,
        user_id: String,
        profit_sol: f64,
        fees_lamports: i64,
        fees_sol: f64,
        roi_pct: f64,
        program_runtime_ms: i64,
    ) -> Result<TradeDataResponse> {
        println!("💾 Saving trade data for user: {}", user_id);

        // Create trade data
        let trade_data = TradeData::new(
            user_id.clone(),
            profit_sol,
            fees_lamports,
            fees_sol,
            roi_pct,
            program_runtime_ms,
        );

        // Save to database
        let saved_trade = self.trade_repo.create(trade_data).await?;
        let trade_response = TradeDataResponse::from(saved_trade);

        println!("✅ Trade data saved for user: {}", user_id);
        Ok(trade_response)
    }

    pub async fn get_user_trades(&self, user_id: &str) -> Result<Vec<TradeDataResponse>> {
        let trades = self.trade_repo.find_by_user_id(user_id).await?;
        let responses: Vec<TradeDataResponse> = trades.into_iter().map(TradeDataResponse::from).collect();
        Ok(responses)
    }

    pub async fn get_recent_trades(&self, limit: i64) -> Result<Vec<TradeDataResponse>> {
        let trades = self.trade_repo.find_recent(limit).await?;
        let responses: Vec<TradeDataResponse> = trades.into_iter().map(TradeDataResponse::from).collect();
        Ok(responses)
    }
}
