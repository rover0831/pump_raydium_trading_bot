use anyhow::Result;
use bson::{doc, oid::ObjectId};
use futures::StreamExt;
use mongodb::{Collection, Database};

use crate::backend::models::bot::BotSettings;

pub struct BotRepository {
    collection: Collection<BotSettings>,
}

impl BotRepository {
    pub fn new(database: Database) -> Self {
        Self {
            collection: database.collection("bot_settings"),
        }
    }

    pub async fn create(&self, mut bot: BotSettings) -> Result<BotSettings> {
        bot.id = Some(ObjectId::new());
        bot.created_at = bson::DateTime::now();
        bot.updated_at = bson::DateTime::now();

        self.collection.insert_one(&bot).await?;

        Ok(bot)
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<BotSettings>> {
        let object_id = ObjectId::parse_str(id)?;
        let filter = doc! { "_id": object_id };
        let bot = self.collection.find_one(filter).await?;

        Ok(bot)
    }

    pub async fn find_by_user_id(&self, user_id: &str) -> Result<Vec<BotSettings>> {
        let filter = doc! { "user_id": user_id };
        let mut cursor = self.collection.find(filter).await?;

        let mut bots = Vec::new();
        while let Some(bot_result) = cursor.next().await {
            let bot = bot_result?;
            bots.push(bot);
        }

        Ok(bots)
    }

    pub async fn update(&self, bot: &BotSettings) -> Result<()> {
        let filter = doc! { "_id": bot.id };
        let update = doc! { "$set": {
            "name": &bot.name,
            "pool_address": &bot.pool_address,
            "buy_sol_amount": bot.buy_sol_amount,
            "entry_percent": bot.entry_percent,
            "entry_slippage": bot.entry_slippage,
            "exit_slippage": bot.exit_slippage,
            "stop_loss": bot.stop_loss,
            "take_profit": bot.take_profit,
            "auto_exit": bot.auto_exit as i64,
            "confirm_service": &bot.confirm_service,
            "cu": bot.cu as i64,
            "priority_fee_micro_lamport": bot.priority_fee_micro_lamport as i64,
            "third_party_fee": bot.third_party_fee,
            "updated_at": bson::DateTime::now()
        }};

        self.collection.update_one(filter, update).await?;

        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let object_id = ObjectId::parse_str(id)?;
        let filter = doc! { "_id": object_id };

        self.collection.delete_one(filter).await?;

        Ok(())
    }
}
