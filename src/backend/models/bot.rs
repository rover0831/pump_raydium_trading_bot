use bson::{DateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Clone, Validate)]
pub struct BotSettings {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: String,
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    // Pool Configuration
    #[validate(length(min = 32, max = 44))]
    pub pool_address: String,

    // Trading Parameters
    #[validate(range(min = 0.0001, max = 1000.0))]
    pub buy_sol_amount: f64,

    #[validate(range(min = 0.0001, max = 100.0))]
    pub entry_percent: f64,

    #[validate(range(min = 0.1, max = 500.0))]
    pub entry_slippage: f64,

    #[validate(range(min = 0.1, max = 1000.0))]
    pub exit_slippage: f64,

    #[validate(range(min = 0.0001, max = 100.0))]
    pub stop_loss: f64,
    
    #[validate(range(min = 0.0001, max = 1000.0))]
    pub take_profit: f64,
    
    #[validate(range(min = 0, max = 86400))]
    pub auto_exit: u64,

    // MEV Service Configuration
    #[validate(length(min = 1, max = 20))]
    pub confirm_service: String, // NOZOMI, JITO, ZSLOT

    // Priority Fee Configuration
    #[validate(range(min = 1, max = 1000000))]
    pub cu: u64,
    #[validate(range(min = 0, max = 1000000))]
    pub priority_fee_micro_lamport: u64,
    #[validate(range(min = 0.0, max = 100.0))]
    pub third_party_fee: f64,

    pub created_at: DateTime,
    pub updated_at: DateTime,
}

impl BotSettings {
    pub fn new(
        user_id: String,
        name: String,
        pool_address: String,
    ) -> Self {
        Self {
            id: None,
            user_id,
            name,
            pool_address,
            buy_sol_amount: 0.001,
            entry_percent: 0.01,
            entry_slippage: 5.0,
            exit_slippage: 100.0,
            stop_loss: 0.01,
            take_profit: 0.01,
            auto_exit: 3600,
            confirm_service: "JITO".to_string(),
            cu: 300000,
            priority_fee_micro_lamport: 20000,
            third_party_fee: 0.0001,
            created_at: DateTime::now(),
            updated_at: DateTime::now(),
        }
    }

    /// Create a default bot for new users
    pub fn create_default_bot(user_id: String) -> Self {
        Self::new(
            user_id,
            "My First Bot".to_string(),
            "".to_string(), // User will set this later
        )
    }

    pub fn update_trading_params(
        &mut self,
        pool_address: Option<String>,
        buy_sol_amount: Option<f64>,
        entry_percent: Option<f64>,
        entry_slippage: Option<f64>,
        exit_slippage: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        auto_exit: Option<u64>,
    ) {
        if let Some(pool_address) = pool_address {
            self.pool_address = pool_address;
        }
        if let Some(amount) = buy_sol_amount {
            self.buy_sol_amount = amount;
        }
        if let Some(percent) = entry_percent {
            self.entry_percent = percent;
        }
        if let Some(slippage) = entry_slippage {
            self.entry_slippage = slippage;
        }
        if let Some(slippage) = exit_slippage {
            self.exit_slippage = slippage;
        }
        if let Some(sl) = stop_loss {
            self.stop_loss = sl;
        }
        if let Some(tp) = take_profit {
            self.take_profit = tp;
        }
        if let Some(ae) = auto_exit {
            self.auto_exit = ae;
        }
        self.updated_at = DateTime::now();
    }

    pub fn update_mev_config(
        &mut self,
        confirm_service: Option<String>,
        cu: Option<u64>,
        priority_fee: Option<u64>,
        third_party_fee: Option<f64>,
    ) {
        if let Some(service) = confirm_service {
            self.confirm_service = service;
        }
        if let Some(compute_units) = cu {
            self.cu = compute_units;
        }
        if let Some(fee) = priority_fee {
            self.priority_fee_micro_lamport = fee;
        }
        if let Some(tpf) = third_party_fee {
            self.third_party_fee = tpf;
        }
        self.updated_at = DateTime::now();
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BotSettingsResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub pool_address: String,
    pub buy_sol_amount: f64,
    pub entry_percent: f64,
    pub entry_slippage: f64,
    pub exit_slippage: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub auto_exit: u64,
    pub confirm_service: String,
    pub cu: u64,
    pub priority_fee_micro_lamport: u64,
    pub third_party_fee: f64,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

impl From<BotSettings> for BotSettingsResponse {
    fn from(bot: BotSettings) -> Self {
        Self {
            id: bot.id.unwrap().to_hex(),
            user_id: bot.user_id,
            name: bot.name,
            pool_address: bot.pool_address,
            buy_sol_amount: bot.buy_sol_amount,
            entry_percent: bot.entry_percent,
            entry_slippage: bot.entry_slippage,
            exit_slippage: bot.exit_slippage,
            stop_loss: bot.stop_loss,
            take_profit: bot.take_profit,
            auto_exit: bot.auto_exit,
            confirm_service: bot.confirm_service,
            cu: bot.cu,
            priority_fee_micro_lamport: bot.priority_fee_micro_lamport,
            third_party_fee: bot.third_party_fee,
            created_at: bot.created_at,
            updated_at: bot.updated_at,
        }
    }
}

impl Default for BotSettingsResponse {
    fn default() -> Self {
        Self {
            id: String::new(),
            user_id: String::new(),
            name: String::new(),
            pool_address: String::new(),
            buy_sol_amount: 0.0,
            entry_percent: 0.0,
            entry_slippage: 0.0,
            exit_slippage: 0.0,
            stop_loss: 0.0,
            take_profit: 0.0,
            auto_exit: 0,
            confirm_service: String::new(),
            cu: 0,
            priority_fee_micro_lamport: 0,
            third_party_fee: 0.0,
            created_at: DateTime::now(),
            updated_at: DateTime::now(),
        }
    }
}
