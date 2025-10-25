use crate::backend::{
    db::bot_repository::BotRepository,
    db::connection::AppDatabase,
    db::user_repository::UserRepository,
    error::{AppError, AppResult},
    models::bot::{BotSettings, BotSettingsResponse},
};
use solana_sdk::instruction::Instruction;
use tracing::info;

pub struct BotService {
    bot_repo: BotRepository,
    user_repo: UserRepository,
}

#[derive(Debug, Clone)]
pub struct UserBotData {
    pub pool_id: String,
    pub user_id: String,
    pub private_key: String,
    pub public_key: String,
    pub bot_setting: BotSettings,
}

#[derive(Debug, Clone)]
pub struct RealPoolInfo {
    pub pool_price: f64,
    pub user_bot_data: UserBotData,
    pub latest_pool_price: f64,
    pub swap_buy_ixs: Vec<Instruction>,
    pub is_bought: bool,
    pub bought_price: Option<f64>,
    pub bought_at: Option<i64>,
    pub initial_wsol_balance: Option<f64>,
    pub signature: Option<String>,
    pub start_time: Option<std::time::Instant>,
    pub last_profit_sol: Option<f64>,
    pub last_input_lamports_delta: Option<i128>,
    pub last_output_lamports_delta: Option<i128>,
    pub last_roi_pct: Option<f64>,
    pub last_duration: Option<std::time::Duration>,
    pub fee: f64,
}

impl BotService {
    pub fn new(database: AppDatabase) -> Self {
        Self {
            bot_repo: BotRepository::new(database.clone()),
            user_repo: UserRepository::new(database.clone()),
        }
    }

    /// Create a new bot for a user
    pub async fn create_bot(
        &self,
        user_id: String,
        name: String,
        pool_address: String,
    ) -> AppResult<BotSettingsResponse> {
        // Check if user already has a bot with this name
        let existing_bots = self.bot_repo.find_by_user_id(&user_id).await?;
        if existing_bots.iter().any(|bot| bot.name == name) {
            return Err(AppError::conflict("Bot name already exists for this user"));
        }

        let bot = BotSettings::new(user_id, name, pool_address);
        let created_bot = self.bot_repo.create(bot).await?;

        info!(
            "✅ New bot created: {} for user: {}",
            created_bot.name, created_bot.user_id
        );

        Ok(created_bot.into())
    }

    /// Get all bots for a user
    pub async fn get_user_bots(&self, user_id: &str) -> AppResult<Vec<BotSettingsResponse>> {
        let bots = self.bot_repo.find_by_user_id(user_id).await?;
        Ok(bots.into_iter().map(|bot| bot.into()).collect())
    }

    /// Update bot trading parameters
    pub async fn update_trading_params(
        &self,
        user_id: &str,
        pool_address: Option<String>,
        buy_sol_amount: Option<f64>,
        buy_usd1_amount: Option<f64>,
        entry_percent: Option<f64>,
        entry_slippage: Option<f64>,
        exit_slippage: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        auto_exit: Option<u64>,
    ) -> AppResult<BotSettingsResponse> {
        let mut bot = self
            .bot_repo
            .find_by_user_id(user_id)
            .await?
            .first()
            .ok_or_else(|| AppError::not_found("Bot not found"))?
            .clone();

        bot.update_trading_params(
            pool_address,
            buy_sol_amount,
            buy_usd1_amount,
            entry_percent,
            entry_slippage,
            exit_slippage,
            stop_loss,
            take_profit,
            auto_exit,
        );

        self.bot_repo.update(&bot).await?;

        info!("✅ Bot trading parameters updated: {}", bot.name);

        Ok(bot.into())
    }

    /// Update bot MEV configuration
    pub async fn update_mev_config(
        &self,
        user_id: &str,
        confirm_service: Option<String>,
        cu: Option<u64>,
        priority_fee: Option<u64>,
        third_party_fee: Option<f64>,
    ) -> AppResult<BotSettingsResponse> {
        let mut bot = self
            .bot_repo
            .find_by_user_id(user_id)
            .await?
            .first()
            .ok_or_else(|| AppError::not_found("Bot not found"))?
            .clone();

        bot.update_mev_config(confirm_service, cu, priority_fee, third_party_fee);

        self.bot_repo.update(&bot).await?;

        info!("✅ Bot MEV configuration updated: {}", bot.name);

        Ok(bot.into())
    }

    /// Simple cleanup function that can be called without self
    async fn simple_cleanup(user_id: &str) {
        println!("🧹 Simple cleanup for user: {}", user_id);
        
        // Remove from USER_LIST with timeout
        {
            let user_list_result = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                crate::statics::USER_LIST.write()
            ).await;
            
            match user_list_result {
                Ok(mut user_list) => {
                    let initial_count = user_list.len();
                    user_list.retain(|existing_user| existing_user.user_id != user_id);
                    let final_count = user_list.len();
                    if initial_count != final_count {
                        println!("🧹 Simple cleanup: Removed {} entries from USER_LIST", initial_count - final_count);
                    }
                },
                Err(_) => {
                    println!("⚠️ USER_LIST write lock timeout, skipping cleanup");
                }
            }
        }
        
        // Remove from REAL_POOL_INFO with timeout
        {
            let real_pool_info_result = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                crate::statics::REAL_POOL_INFO.write()
            ).await;
            
            match real_pool_info_result {
                Ok(mut real_pool_info) => {
                    real_pool_info.retain(|_pool_id, pool_infos| {
                        let initial_users = pool_infos.len();
                        pool_infos.retain(|pool_info| pool_info.user_bot_data.user_id != user_id);
                        let final_users = pool_infos.len();
                        if initial_users != final_users {
                            println!("🧹 Simple cleanup: Removed {} entries from pool", initial_users - final_users);
                        }
                        !pool_infos.is_empty()
                    });
                },
                Err(_) => {
                    println!("⚠️ REAL_POOL_INFO write lock timeout, skipping cleanup");
                }
            }
        }
        println!("🧹 Simple cleanup completed for user: {}", user_id);
    }

    pub async fn start_bot(&self, user_id: &str) -> AppResult<String> {
        println!("🚀 Starting bot for user_id: {}", user_id);
        
        // Simple cleanup attempt with timeout
        println!("🧹 Attempting quick cleanup...");
        let user_id_clone = user_id.to_string();
        let cleanup_result = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            Self::simple_cleanup(&user_id_clone)
        ).await;
        
        match cleanup_result {
            Ok(_) => println!("✅ Quick cleanup completed"),
            Err(_) => println!("⚠️ Quick cleanup timed out, continuing..."),
        }
        
        // Check if user already exists in USER_LIST (quick check)
        {
            let user_list = crate::statics::USER_LIST.read().await;
            if user_list.iter().any(|u| u.user_id == user_id) {
                println!("⚠️ User {} already exists in USER_LIST, will be replaced", user_id);
            }
        }
        
        // Check if user exists in database
        let user = match self.user_repo.find_by_id(user_id).await? {
            Some(user) => {
                println!("✅ User found: {}", user_id);
                user
            },
            None => {
                println!("❌ User not found: {}", user_id);
                return Err(AppError::not_found("User not found"));
            }
        };

        // Check if bot exists
        let bot = self.bot_repo.find_by_user_id(user_id).await?;
        if bot.is_empty() {
            println!("❌ No bot found for user: {}", user_id);
            return Err(AppError::not_found("Bot not found"));
        }
        
        let bot_settings = bot.first().unwrap();
        let pool_id = bot_settings.pool_address.clone();
        println!("✅ Bot found with pool_id: {}", pool_id);

        // Create UserBotData and add to USER_LIST
        let user_bot_data = UserBotData {
            pool_id: pool_id.clone(),
            user_id: user_id.to_string(),
            private_key: user.private_key.clone(),
            public_key: user.public_key.clone(),
            bot_setting: bot_settings.clone(),
        };

        println!("USER_BOT_DATA: {:#?}", user_bot_data);

        // Add to USER_LIST (with fallback cleanup if needed)
        {
            let user_list_result = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                crate::statics::USER_LIST.write()
            ).await;
            
            match user_list_result {
                Ok(mut user_list) => {
                    // Remove any existing entries for this user (fallback)
                    let initial_count = user_list.len();
                    user_list.retain(|existing_user| existing_user.user_id != user_id);
                    let final_count = user_list.len();
                    if initial_count != final_count {
                        println!("🧹 Fallback: Removed {} existing entries from USER_LIST", initial_count - final_count);
                    }
                    
                    user_list.push(user_bot_data.clone());
                    println!("✅ USER_LIST: Added user, total users: {}", user_list.len());
                },
                Err(_) => {
                    println!("⚠️ Could not acquire USER_LIST write lock, trying alternative approach...");
                    
                    // Try to add in background
                    let user_bot_data_clone = user_bot_data.clone();
                    let user_id_clone = user_id.to_string();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        if let Ok(mut user_list) = crate::statics::USER_LIST.try_write() {
                            user_list.retain(|existing_user| existing_user.user_id != user_id_clone);
                            user_list.push(user_bot_data_clone);
                            println!("✅ USER_LIST: Added user in background");
                        }
                    });
                    println!("⚠️ USER_LIST: Will be added in background");
                }
            }
        }

        // Create initial RealPoolInfo and add to REAL_POOL_INFO
        println!("🔧 Creating RealPoolInfo...");
        
        // Try to get write lock with timeout
        let real_pool_info_result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            crate::statics::REAL_POOL_INFO.write()
        ).await;
        
        match real_pool_info_result {
            Ok(mut real_pool_info) => {
                println!("🔧 Got write lock on REAL_POOL_INFO");
                
                // Fallback cleanup: remove any existing entries for this user
                real_pool_info.retain(|_pool_id, pool_infos| {
                    let initial_users = pool_infos.len();
                    pool_infos.retain(|pool_info| pool_info.user_bot_data.user_id != user_id);
                    let final_users = pool_infos.len();
                    if initial_users != final_users {
                        println!("🧹 Fallback: Removed {} existing entries from pool", initial_users - final_users);
                    }
                    !pool_infos.is_empty() // Keep the pool entry only if it has remaining users
                });
                
                println!("🔧 Creating RealPoolInfo struct...");
                let initial_pool_info = RealPoolInfo {
                    pool_price: 0.0,
                    user_bot_data: user_bot_data.clone(),
                    latest_pool_price: 0.0,
                    swap_buy_ixs: vec![],
                    is_bought: false,
                    bought_price: None,
                    bought_at: None,
                    initial_wsol_balance: Some(0.0),
                    signature: None,
                    start_time: Some(std::time::Instant::now()),
                    last_profit_sol: None,
                    last_input_lamports_delta: None,
                    last_output_lamports_delta: None,
                    last_roi_pct: None,
                    last_duration: None,
                    fee: 0.01,
                };
                println!("🔧 RealPoolInfo created successfully");

                if let Some(pool_infos) = real_pool_info.get_mut(&pool_id) {
                    // Pool exists, add user
                    pool_infos.push(initial_pool_info);
                    println!("✅ REAL_POOL_INFO (existing pool): Added user, total users: {}", pool_infos.len());
                } else {
                    // Pool doesn't exist, create new pool entry
                    real_pool_info.insert(pool_id, vec![initial_pool_info]);
                    println!("✅ REAL_POOL_INFO (new pool): Created new pool with 1 user");
                }
            },
            Err(_) => {
                println!("⚠️ Could not acquire REAL_POOL_INFO write lock, trying alternative approach...");
                
                // Alternative: try to add without cleanup (let the system handle duplicates)
                let initial_pool_info = RealPoolInfo {
                    pool_price: 0.0,
                    user_bot_data: user_bot_data.clone(),
                    latest_pool_price: 0.0,
                    swap_buy_ixs: vec![],
                    is_bought: false,
                    bought_price: None,
                    bought_at: None,
                    initial_wsol_balance: Some(0.0),
                    signature: None,
                    start_time: Some(std::time::Instant::now()),
                    last_profit_sol: None,
                    last_input_lamports_delta: None,
                    last_output_lamports_delta: None,
                    last_roi_pct: None,
                    last_duration: None,
                    fee: 0.01,
                };
                
                // Try to add in background
                let pool_id_clone = pool_id.clone();
                let user_id_clone = user_id.to_string();
                tokio::spawn(async move {
                    // Wait a bit and try again
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    if let Ok(mut real_pool_info) = crate::statics::REAL_POOL_INFO.try_write() {
                        // Clean up first
                        real_pool_info.retain(|_pool_id, pool_infos| {
                            pool_infos.retain(|pool_info| pool_info.user_bot_data.user_id != user_id_clone);
                            !pool_infos.is_empty()
                        });
                        
                        // Add new entry
                        if let Some(pool_infos) = real_pool_info.get_mut(&pool_id_clone) {
                            pool_infos.push(initial_pool_info);
                        } else {
                            real_pool_info.insert(pool_id_clone, vec![initial_pool_info]);
                        }
                        println!("✅ REAL_POOL_INFO: Added user in background");
                    }
                });
                println!("⚠️ REAL_POOL_INFO: Will be added in background");
            }
        }

        Ok("Started bot".to_string())
    }

    pub async fn stop_bot(&self, user_id: &str) -> AppResult<String> {
        println!("🛑 stop_bot called for user_id: {}", user_id);
        let mut is_bought = false;
        let bot = self.bot_repo.find_by_user_id(user_id).await?;
        let bot_settings = bot.first().unwrap();
        let pool_id = bot_settings.pool_address.clone();

        {
            let mut real_pool_info = crate::statics::REAL_POOL_INFO.write().await;
            if let Some(pool_info) = real_pool_info.get_mut(&pool_id) {
                for info in pool_info {
                    if info.user_bot_data.user_id.to_string() == user_id.to_string() {
                        is_bought = info.is_bought;
                    }
                }
            }
        } // Write lock is dropped here
        println!("IS_BOUGHT: {}", is_bought);

        if !is_bought {
            // Remove from USER_LISTz
            let mut user_list = crate::statics::USER_LIST.write().await;
            if user_list
                .iter()
                .any(|user_bot_data| user_bot_data.user_id == user_id)
            {
                user_list.retain(|user_bot_data| user_bot_data.user_id != user_id);
            }

            // Remove from REAL_POOL_INFO if it has data
            {
                let mut real_pool_info = crate::statics::REAL_POOL_INFO.write().await;
                println!("REAL_POOL_INFO length: {}", real_pool_info.len());
                println!("REAL_POOL_INFO keys: {:?}", real_pool_info.keys().collect::<Vec<_>>());
                if real_pool_info.len() > 0 {
                    real_pool_info.retain(|_pool_id, pool_infos| {
                        pool_infos.retain(|pool_info| pool_info.user_bot_data.user_id != user_id);
                        !pool_infos.is_empty() // Keep the pool entry only if it has remaining users
                    });
                }
            }

            info!("✅ Bot stopped for user: {}", user_id);
            return Ok("Stopped bot".to_string());
        } else {
            // Bot has bought tokens, need to sell them first
            let mut real_pool_info = crate::statics::REAL_POOL_INFO.write().await;
            println!("🔄 Bot stopping - triggering sell for user: {}", user_id);
            if real_pool_info.len() > 0 {
                if let Some(pool_info) = real_pool_info.get_mut(&pool_id) {
                    for info in pool_info {
                        if info.user_bot_data.user_id.to_string() == user_id {
                            // Set auto_exit to 0 to trigger immediate sell
                            info.user_bot_data.bot_setting.auto_exit = 0;
                        }
                    }
                }
            }

            // let mut user_list = crate::statics::USER_LIST.write().await;

            // user_list.retain(|user_bot_data| user_bot_data.user_id != user_id);
            // info!(
            //     "🔄 Bot stopping - removing user from USER_LIST: {}",
            //     user_id
            // );

            return Ok("Stopped bot".to_string());
        }
    }
}
