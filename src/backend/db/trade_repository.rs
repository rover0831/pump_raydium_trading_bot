use bson::{doc, oid::ObjectId};
use mongodb::{Collection, Database};
use anyhow::Result;

use crate::backend::models::trade::TradeData;

pub struct TradeRepository {
    collection: Collection<TradeData>,
}

impl TradeRepository {
    pub fn new(database: Database) -> Self {
        Self {
            collection: database.collection("trade_data"),
        }
    }

    pub async fn create(&self, mut trade: TradeData) -> Result<TradeData> {
        trade.id = Some(ObjectId::new());
        trade.created_at = bson::DateTime::now();
        
        self.collection.insert_one(&trade).await?;
        
        Ok(trade)
    }

    pub async fn find_by_user_id(&self, user_id: &str) -> Result<Vec<TradeData>> {
        let filter = doc! { "user_id": user_id };
        let mut cursor = self.collection.find(filter).await?;
        
        let mut trades = Vec::new();
        while cursor.advance().await? {
            trades.push(cursor.deserialize_current()?);
        }
        
        Ok(trades)
    }

    pub async fn find_recent(&self, limit: i64) -> Result<Vec<TradeData>> {
        let mut cursor = self.collection.find(doc! {}).await?;
        
        let mut trades = Vec::new();
        let mut count = 0;
        while cursor.advance().await? && count < limit {
            trades.push(cursor.deserialize_current()?);
            count += 1;
        }
        
        Ok(trades)
    }

    pub async fn get_stats(&self, user_id: Option<&str>) -> Result<TradeStats> {
        let filter = if let Some(user_id) = user_id {
            doc! { "user_id": user_id }
        } else {
            doc! {}
        };

        let pipeline = vec![
            doc! {
                "$match": filter
            },
            doc! {
                "$group": {
                    "_id": null,
                    "total_trades": { "$sum": 1 },
                    "total_profit": { "$sum": "$profit_sol" },
                    "avg_roi": { "$avg": "$roi_pct" },
                    "profitable_trades": {
                        "$sum": {
                            "$cond": [{ "$gt": ["$profit_sol", 0] }, 1, 0]
                        }
                    }
                }
            }
        ];

        let mut cursor = self.collection.aggregate(pipeline).await?;
        let mut stats = TradeStats::default();

        if cursor.advance().await? {
            let doc = cursor.deserialize_current()?;
            stats.total_trades = doc.get_i32("total_trades").unwrap_or(0) as u32;
            stats.total_profit = doc.get_f64("total_profit").unwrap_or(0.0);
            stats.avg_roi = doc.get_f64("avg_roi").unwrap_or(0.0);
            stats.profitable_trades = doc.get_i32("profitable_trades").unwrap_or(0) as u32;
            stats.win_rate = if stats.total_trades > 0 {
                (stats.profitable_trades as f64 / stats.total_trades as f64) * 100.0
            } else {
                0.0
            };
        }

        Ok(stats)
    }
}

#[derive(Debug, Default)]
pub struct TradeStats {
    pub total_trades: u32,
    pub total_profit: f64,
    pub avg_roi: f64,
    pub profitable_trades: u32,
    pub win_rate: f64,
}
