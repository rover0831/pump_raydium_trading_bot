use {
    async_trait::async_trait,
    base64, bincode,
    carbon_core::{
        deserialize::ArrangeAccounts,
        error::CarbonResult,
        instruction::{DecodedInstruction, InstructionProcessorInputType},
        metrics::MetricsCollection,
        processor::Processor,
    },
    carbon_log_metrics::LogMetrics,
    carbon_pump_swap_decoder::{
        instructions::{buy::Buy, sell::Sell, PumpSwapInstruction},
        PumpSwapDecoder, PROGRAM_ID as PUMPSWAP_PROGRAM_ID,
    },
    carbon_raydium_amm_v4_decoder::{
        instructions::{swap_base_in::SwapBaseIn, RaydiumAmmV4Instruction},
        RaydiumAmmV4Decoder, PROGRAM_ID as RAY_V4_PROGRAM_ID,
    },
    carbon_yellowstone_grpc_datasource::YellowstoneGrpcGeyserClient,
    raydium_amm_monitor::{
        backend::server::start_backend_server,
        config::{init_jito, init_nozomi, init_zslot, JITO_CLIENT, RPC_CLIENT},
        instructions::{
            buy::BuyInstructionAccountsExt, sell::SellInstructionAccountsExt,
            SwapBaseInInstructionAccountsExt,
        },
        service::Tips,
        utils::{
            blockhash::{get_slot, recent_blockhash_handler, WSOL},
            build_and_sign::build_and_sign,
            parse::get_coin_pc_mint,
            swap_quote::sol_token_quote,
        },
    },
    serde_json::json,
    solana_client::{
        rpc_config::RpcSimulateTransactionConfig, rpc_response::RpcSimulateTransactionResult,
    },
    solana_sdk::{
        commitment_config::CommitmentConfig, instruction::Instruction, pubkey::Pubkey,
        transaction::VersionedTransaction,
    },
    solana_transaction_status_client_types::UiTransactionEncoding,
    spl_associated_token_account::get_associated_token_address,
    std::{
        collections::{HashMap, HashSet},
        env,
        sync::Arc,
        time::Duration,
    },
    tokio::{sync::RwLock, time::sleep},
    yellowstone_grpc_proto::geyser::{CommitmentLevel, SubscribeRequestFilterTransactions},
};

use chrono::Utc;
use mongodb::{bson::doc, options::ClientOptions, Client};
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Keypair;

#[derive(Debug, Serialize, Deserialize)]
struct TradeData {
    user_id: String,
    ts: String,
    profit_sol: f64,
    fees_lamports: i64,
    fees_sol: f64,
    roi_pct: f64,
    program_runtime_ms: i64,
}

/// Simulate a transaction to check if it would succeed
async fn simulate_transaction(
    transaction: &VersionedTransaction,
) -> Result<RpcSimulateTransactionResult, Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Starting transaction simulation...");

    let simulation_result = RPC_CLIENT
        .simulate_transaction_with_config(
            transaction,
            RpcSimulateTransactionConfig {
                sig_verify: false,
                replace_recent_blockhash: true,
                commitment: Some(CommitmentConfig::processed()),
                encoding: Some(UiTransactionEncoding::Base64),
                accounts: None,
                min_context_slot: None,
                inner_instructions: true,
            },
        )
        .await?;

    log::info!(
        "Simulation completed with result: {:?}",
        simulation_result.value
    );

    Ok(simulation_result.value)
}

#[tokio::main]
pub async fn main() -> CarbonResult<()> {
    dotenv::dotenv().ok();

    // Initialize logging only once
    env_logger::init();

    // Spawn backend server with proper error handling
    tokio::spawn(async move {
        match start_backend_server().await {
            Ok(_) => println!("Backend server completed successfully"),
            Err(e) => eprintln!("Backend server failed: {}", e),
        }
    });

    // Give the backend server a moment to start up
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Check if backend server is running
    match reqwest::get("http://0.0.0.0:3000/").await {
        Ok(response) => {
            if response.status().is_success() {
                println!("✅ Backend server is running successfully on http://0.0.0.0:3000");
                println!("   API endpoints available at http://0.0.0.0:3000");
            } else {
                println!(
                    "⚠️  Backend server responded with status: {}",
                    response.status()
                );
            }
        }
        Err(e) => {
            println!("❌ Backend server health check failed: {}", e);
            println!("   Make sure MongoDB is running and environment variables are set correctly");
            println!("   Frontend will not be able to connect to backend");
        }
    }

    // init_nozomi().await;
    // init_zslot().await;
    init_jito().await;

    // {
    //     let mut start_time_guard = START_TIME.write().await;
    //     *start_time_guard = Some(std::time::Instant::now());
    //     println!("Start time: {:?}", *start_time_guard);
    // }
    tokio::spawn({
        async move {
            loop {
                recent_blockhash_handler(RPC_CLIENT.clone()).await;
            }
        }
    });

    // NOTE: Workaround, that solving issue https://github.com/rustls/rustls/issues/1877
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Can't set crypto provider to aws_lc_rs");

    // Spawn a task to monitor pool price changes when REAL_POOL_INFO length changes
    tokio::spawn(async move {
        loop {
            // Get a snapshot of the data to avoid deadlocks
            let pool_data = {
                let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                real_pool_info.clone() // Clone the data to release the read lock
            };

            for (pool_id, pool_infos) in pool_data.iter() {
                println!("Starting monitoring for pool_id: {}", pool_id);

                for pool_info in pool_infos {
                    let pool_price = pool_info.pool_price;
                    let latest_price = pool_info.latest_pool_price;

                    // Only process if we have a price change
                    if latest_price > 0.0 {
                        println!("✅ Condition met! Calling display_pool_price_change");
                        let pool_info_clone = pool_info.clone();
                        let latest = pool_price.clone();
                        let latest_val = latest_price.clone();

                        // Update the pool_price to match latest_price before processing
                        {
                            let mut real_pool_info_write =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                            if let Some(pool_infos) = real_pool_info_write.get_mut(pool_id) {
                                for info in pool_infos {
                                    if info.user_bot_data.user_id.to_string()
                                        == pool_info.user_bot_data.user_id.to_string()
                                    {
                                        info.pool_price = latest_val;
                                    }
                                }
                            }
                        } // Write lock is automatically dropped here

                        // Now process the price change with updated data
                        display_pool_price_change(latest, latest_val, pool_info_clone.clone())
                            .await;
                    } else {
                        println!("❌ Condition NOT met - display_pool_price_change NOT called");
                    }
                }
            }
            // Check for new pools every 1s
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    });

    let transaction_filter = SubscribeRequestFilterTransactions {
        vote: Some(false),
        failed: Some(false),
        account_include: vec![
            RAY_V4_PROGRAM_ID.to_string().clone(),
            PUMPSWAP_PROGRAM_ID.to_string().clone(),
        ],
        account_exclude: vec![],
        account_required: vec![],
        signature: None,
    };

    let mut transaction_filters: HashMap<String, SubscribeRequestFilterTransactions> =
        HashMap::new();

    transaction_filters.insert(
        "ray_pumpswap_transaction_filter".to_string(),
        transaction_filter,
    );

    let yellowstone_grpc = YellowstoneGrpcGeyserClient::new(
        env::var("GEYSER_URL").unwrap_or_default(),
        env::var("X_TOKEN").ok(),
        Some(CommitmentLevel::Processed),
        HashMap::new(),
        transaction_filters,
        Default::default(),
        Arc::new(RwLock::new(HashSet::new())),
    );

    println!("Starting RAYDIUM V4 Monitor...");

    carbon_core::pipeline::Pipeline::builder()
        .datasource(yellowstone_grpc)
        .metrics(Arc::new(LogMetrics::new()))
        .metrics_flush_interval(3)
        .instruction(RaydiumAmmV4Decoder, RaydiumV4Process)
        .instruction(PumpSwapDecoder, PumpSwapProcess)
        .shutdown_strategy(carbon_core::pipeline::ShutdownStrategy::Immediate)
        .build()?
        .run()
        .await?;

    println!("Raydium Launchpad and PumpSwap Monitor has stopped.");

    Ok(())
}

async fn display_pool_price_change(
    old: f64,
    new: f64,
    pool_info: raydium_amm_monitor::backend::services::bot_service::RealPoolInfo,
) {
    if old > 0.0 {
        tokio::spawn({
            let bought = pool_info.is_bought;

            async move {
                if !bought {
                    let new_clone = new.clone();
                    // Fix division by zero bug - ensure old price is not zero
                    let percent = if old > 0.0 {
                        ((old - new_clone) / old) * 100.0
                    } else {
                        0.0 // Default to 0% if old price is zero or negative
                    };
                    println!(
                        "POOL_PRICE changed: old = {:.8}, new = {:.8}, change = {:+.4}%",
                        old, new_clone, percent
                    );
                    if percent <= -pool_info.user_bot_data.bot_setting.entry_percent {
                        println!(
                            "ALERT: POOL_PRICE dropped more than {}%!",
                            pool_info.user_bot_data.bot_setting.entry_percent
                        );
                        let pool_info_for_spawn = pool_info.clone();
                        let new_price_clone = new_clone.clone();
                        let current_time = Utc::now().timestamp_millis();
                        {
                            let mut real_pool_info =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                            let pool_info = real_pool_info
                                .get_mut(&pool_info_for_spawn.user_bot_data.pool_id.clone())
                                .unwrap();
                            for info in pool_info {
                                if info.user_bot_data.user_id.to_string()
                                    == pool_info_for_spawn.user_bot_data.user_id.to_string()
                                {
                                    info.bought_price = Some(new_price_clone);
                                    info.bought_at = Some(current_time);
                                }
                            }
                        }

                        match build_and_submit_swap_transaction(pool_info_for_spawn.clone()).await {
                            Ok(result) => log::info!("Transaction result: {:?}", result),
                            Err(err) => log::error!("Transaction failed: {}", err),
                        }
                    }
                } else {
                    let new_clone = new.clone();
                    let bought_price = pool_info.bought_price;
                    let old_bought_price = bought_price.clone();
                    // Fix division by zero bug - ensure old_bought_price is not zero
                    let percent = if let Some(old_price) = old_bought_price {
                        if old_price > 0.0 {
                            ((new_clone - old_price) / old_price) * 100.0
                        } else {
                            0.0 // Default to 0% if old price is zero or negative
                        }
                    } else {
                        0.0 // Default to 0% if no bought price available
                    };
                    if let Some(old_price) = old_bought_price {
                        println!(
                            "POOL_PRICE changed : buy price = {:.8}, current price = {:.8}, change = {:.8}",
                            old_price, new_clone, percent
                        );
                    }
                    let bought_at = pool_info.bought_at;
                    let current_time = Utc::now().timestamp_millis();

                    // Check how long it flowed from bought_at
                    if percent >= pool_info.user_bot_data.bot_setting.take_profit {
                        println!("ALERT: POOL_PRICE increased more than {}%!", percent);
                        // Reset IS_BOUGHT to false when selling
                        match build_and_submit_swap_transaction(pool_info.clone()).await {
                            Ok(result) => {
                                log::info!("Take profit transaction result: {:?}", result)
                            }
                            Err(err) => log::error!("Take profit transaction failed: {}", err),
                        }

                        // Clean up bot state after selling
                        cleanup_bot_after_sell(&pool_info).await;
                    } else if percent <= -pool_info.user_bot_data.bot_setting.stop_loss {
                        println!("ALERT: POOL_PRICE decreased more than {}%!", percent);

                        // Reset IS_BOUGHT to false when selling
                        match build_and_submit_swap_transaction(pool_info.clone()).await {
                            Ok(result) => log::info!("Stop loss transaction result: {:?}", result),
                            Err(err) => log::error!("Stop loss transaction failed: {}", err),
                        }

                        // Clean up bot state after selling
                        cleanup_bot_after_sell(&pool_info).await;
                    } else if pool_info.user_bot_data.bot_setting.auto_exit == 0 {
                        // Immediate sell triggered by stop_bot
                        println!("ALERT: IMMEDIATE SELL triggered by stop_bot!");
                        match build_and_submit_swap_transaction(pool_info.clone()).await {
                            Ok(result) => {
                                log::info!("Immediate sell transaction result: {:?}", result)
                            }
                            Err(err) => log::error!("Immediate sell transaction failed: {}", err),
                        }

                        // Clean up bot state after selling
                        cleanup_bot_after_sell(&pool_info).await;
                    } else if let Some(bought_at_time) = bought_at {
                        // Fix unsafe unwrap() by using safer conversion
                        let auto_exit_ms =
                            match (pool_info.user_bot_data.bot_setting.auto_exit * 1000).try_into()
                            {
                                Ok(ms) => ms,
                                Err(_) => {
                                    println!(
                                        "Warning: AUTO_EXIT conversion failed, using default 1000ms"
                                    );
                                    1000i64
                                }
                            };
                        if current_time - bought_at_time > auto_exit_ms {
                            println!(
                                "ALERT: AUTO EXIT triggered after {} seconds!",
                                pool_info.user_bot_data.bot_setting.auto_exit
                            );
                            // Reset IS_BOUGHT to false when selling
                            match build_and_submit_swap_transaction(pool_info.clone()).await {
                                Ok(result) => {
                                    log::info!("Auto exit transaction result: {:?}", result)
                                }
                                Err(err) => log::error!("Auto exit transaction failed: {}", err),
                            }

                            // Clean up bot state after selling
                            cleanup_bot_after_sell(&pool_info).await;
                        }
                    }
                }
            }
        });
    }
}

async fn build_and_submit_swap_transaction(
    pool_info: raydium_amm_monitor::backend::services::bot_service::RealPoolInfo,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    // let (cu, priority_fee_micro_lamport, third_party_fee) = *PRIORITY_FEE;
    let keypair = Keypair::from_base58_string(&pool_info.user_bot_data.private_key);
    let start = std::time::Instant::now();

    // Print current timestamp and consumed time from start
    println!("Submitting tx --> Current time: {:#?}", Utc::now());

    let pool_info_clone = pool_info.clone();
    let buy_ixs = {
        let buy_ixs_guard = pool_info_clone.swap_buy_ixs;
        buy_ixs_guard.clone()
    };

    if buy_ixs.is_empty() {
        println!("No swap instructions to submit.");
        return Ok(json!({ "result": "error", "message": "No swap instructions to submit" }));
    }

    let results = match pool_info.user_bot_data.bot_setting.confirm_service.as_str() {
        // "NOZOMI" => {
        //     let nozomi = match NOZOMI_CLIENT.get() {
        //         Some(client) => client,
        //         None => {
        //             println!("Error: Nozomi client not initialized");
        //             return;
        //         }
        //     };

        //     let ixs = nozomi.add_tip_ix(Tips {
        //         cu: Some(pool_info.user_bot_data.bot_setting.cu),
        //         priority_fee_micro_lamport: Some(pool_info.user_bot_data.bot_setting.priority_fee_micro_lamport),
        //         payer: pool_info.user_bot_data.public_key.parse::<Pubkey>().clone().unwrap(),
        //         pure_ix: buy_ixs.clone(),
        //         tip_addr_idx: 1,
        //         tip_sol_amount: pool_info.user_bot_data.bot_setting.third_party_fee,
        //     });

        //     let recent_blockhash = get_slot();

        //     let encoded_tx = build_and_sign(ixs, recent_blockhash, None);

        //     match nozomi.send_transaction(&encoded_tx).await {
        //         Ok(data) => {
        //             // Extract signature from the result
        //             pool_info.signature =
        //                 Some(data["result"].as_str().unwrap_or_default().to_string());
        //             json!({ "result": data })
        //         }
        //         Err(err) => {
        //             json!({ "result": "error", "message": err.to_string() })
        //         }
        //     }
        // }
        // "ZERO_SLOT" => {
        //     let zero_slot = match ZSLOT_CLIENT.get() {
        //         Some(client) => client,
        //         None => {
        //             println!("Error: ZSlot client not initialized");
        //             return;
        //         }
        //     };

        //     let ixs = zero_slot.add_tip_ix(Tips {
        //         cu: Some(pool_info.user_bot_data.bot_setting.cu),
        //         priority_fee_micro_lamport: Some(
        //             pool_info
        //                 .user_bot_data
        //                 .bot_setting
        //                 .priority_fee_micro_lamport,
        //         ),
        //         payer: pool_info
        //             .user_bot_data
        //             .public_key
        //             .parse::<Pubkey>()
        //             .clone()
        //             .unwrap(),
        //         pure_ix: buy_ixs,
        //         tip_addr_idx: 1,
        //         tip_sol_amount: pool_info.user_bot_data.bot_setting.third_party_fee,
        //     });

        //     let recent_blockhash = get_slot();

        //     let encoded_tx = build_and_sign(
        //         ixs,
        //         recent_blockhash,
        //         None,
        //         pool_info
        //             .user_bot_data
        //             .public_key
        //             .parse::<Pubkey>()
        //             .clone()
        //             .unwrap(),
        //         keypair,
        //     );

        //     match zero_slot.send_transaction(&encoded_tx).await {
        //         Ok(data) => {
        //             // Extract signature from the result
        //             pool_info.signature =
        //                 Some(data["result"].as_str().unwrap_or_default().to_string());
        //             json!({ "result": data })
        //         }
        //         Err(err) => {
        //             json!({ "result": "error", "message": err.to_string() })
        //         }
        //     }
        // }
        "JITO" => {
            let jito = match JITO_CLIENT.get() {
                Some(client) => client,
                None => {
                    println!("Error: Jito client not initialized");
                    return Ok(
                        json!({ "result": "error", "message": "Jito client not initialized" }),
                    );
                }
            };

            let ixs = jito.add_tip_ix(Tips {
                cu: Some(pool_info.user_bot_data.bot_setting.cu),
                priority_fee_micro_lamport: Some(
                    pool_info
                        .user_bot_data
                        .bot_setting
                        .priority_fee_micro_lamport,
                ),
                payer: pool_info
                    .user_bot_data
                    .public_key
                    .parse::<Pubkey>()
                    .clone()
                    .unwrap(),
                pure_ix: buy_ixs,
                tip_addr_idx: 4,
                tip_sol_amount: pool_info.user_bot_data.bot_setting.third_party_fee,
            });

            let recent_blockhash = get_slot();

            let encoded_tx = build_and_sign(
                ixs,
                recent_blockhash,
                None,
                pool_info
                    .user_bot_data
                    .public_key
                    .parse::<Pubkey>()
                    .clone()
                    .unwrap(),
                keypair,
            );

            // Simulate transaction before sending
            log::info!(
                "Simulating transaction for pool: {}",
                pool_info.user_bot_data.pool_id
            );

            // Parse the encoded transaction for simulation
            let transaction_bytes = base64::decode(&encoded_tx)
                .map_err(|e| format!("Failed to decode transaction: {}", e))?;
            let transaction: VersionedTransaction = bincode::deserialize(&transaction_bytes)
                .map_err(|e| format!("Failed to deserialize transaction: {}", e))?;

            match simulate_transaction(&transaction).await {
                Ok(simulation_result) => {
                    log::info!("=== TRANSACTION SIMULATION RESULTS ===");
                    log::info!("Pool ID: {}", pool_info.user_bot_data.pool_id);
                    log::info!("User ID: {}", pool_info.user_bot_data.user_id);
                    log::info!(
                        "Estimated compute units: {}",
                        simulation_result.units_consumed.unwrap_or(0)
                    );
                    log::info!("Simulation successful: {}", simulation_result.err.is_none());

                    if let Some(logs) = simulation_result.logs {
                        log::info!("Simulation logs ({} entries):", logs.len());
                        for (i, log_entry) in logs.iter().enumerate() {
                            log::info!("  [{}] {}", i + 1, log_entry);
                        }
                    }

                    if let Some(accounts) = simulation_result.accounts {
                        log::info!("Account changes: {} accounts modified", accounts.len());
                    }

                    log::info!("=== END SIMULATION RESULTS ===");

                    // Check if simulation failed
                    if simulation_result.err.is_some() {
                        log::error!("Transaction simulation failed: {:?}", simulation_result.err);
                        return Ok(
                            json!({ "result": "simulation_error", "message": format!("Simulation failed: {:?}", simulation_result.err) }),
                        );
                    }
                }
                Err(err) => {
                    log::error!("Failed to simulate transaction: {}", err);
                    return Ok(
                        json!({ "result": "simulation_error", "message": format!("Simulation error: {}", err) }),
                    );
                }
            }

            match jito.send_transaction(&encoded_tx).await {
                Ok(data) => {
                    // Extract signature from the result
                    {
                        let user_id = pool_info.user_bot_data.user_id.clone();
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_infos = real_pool_info
                            .get_mut(&pool_info.user_bot_data.pool_id.clone())
                            .unwrap();
                        for info in pool_infos {
                            if info.user_bot_data.user_id.to_string() == user_id {
                                info.signature =
                                    Some(data["result"].as_str().unwrap_or_default().to_string());
                            }
                        }
                    }
                    Ok(json!({ "result": data }))
                }
                Err(err) => Ok(json!({ "result": "error", "message": err.to_string() })),
            }
        }
        _ => Ok(json!({ "result": "error", "message": "unknown confirmation service" })),
    };

    println!(
        "Transaction submitting --> : {:#?}\nCurrent time: {:#?}\nPeriod from start: {:?}",
        results,
        Utc::now(),
        start.elapsed()
    );

    results
}

async fn save_trade_metrics(
    user_id: String,
    profit_sol: f64,
    total_fees_lamports_f64: f64,
    roi_pct: f64,
    duration_ms: i64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get database connection from backend
    let uri = std::env::var("MONGODB_URI").expect("MONGODB_URI not set");
    let options = ClientOptions::parse(uri).await?;
    let client = Client::with_options(options)?;
    let database = client.database("trading"); // Use same database as backend

    // Create trade service
    let trade_service =
        raydium_amm_monitor::backend::services::trade_service::TradeService::new(database);

    // Save trade data using backend service
    let _trade_response = trade_service
        .save_trade_data(
            user_id.clone(),
            profit_sol,
            total_fees_lamports_f64 as i64,
            total_fees_lamports_f64 / 1_000_000_000.0,
            roi_pct,
            duration_ms,
        )
        .await?;

    println!("✅ Trade metrics saved for user: {}", user_id);
    Ok(())
}
pub struct RaydiumV4Process;
pub struct PumpSwapProcess;

#[async_trait]
impl Processor for RaydiumV4Process {
    type InputType = InstructionProcessorInputType<RaydiumAmmV4Instruction>;

    async fn process(
        &mut self,
        (metadata, instruction, _nested_instructions, _instructions): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        let user_list = raydium_amm_monitor::statics::USER_LIST.read().await;
        let user_list_clone = user_list.clone();
        drop(user_list); // Release the read lock immediately

        // Process all users concurrently without blocking
        for user_bot_data in user_list_clone.iter() {
            // Check if this user's pool_id matches the current transaction
            let pool_address = match user_bot_data.pool_id.parse::<Pubkey>() {
                Ok(pubkey) => pubkey,
                Err(_) => continue, // Skip invalid pool addresses
            };

            // Check if the pool is involved in this transaction
            let static_account_keys = metadata.transaction_metadata.message.static_account_keys();
            let writable_account_keys =
                &metadata.transaction_metadata.meta.loaded_addresses.writable;
            let readonly_account_keys =
                &metadata.transaction_metadata.meta.loaded_addresses.readonly;

            let mut account_keys: Vec<Pubkey> = vec![];
            account_keys.extend(static_account_keys);
            account_keys.extend(writable_account_keys);
            account_keys.extend(readonly_account_keys);

            if !account_keys.contains(&pool_address) || account_keys.contains(&PUMPSWAP_PROGRAM_ID) {
                continue; // Skip users whose pool is not involved in this transaction
            }

            let metadata_clone = metadata.clone();
            let instruction_clone = instruction.clone();
            let user_bot_data_clone = user_bot_data.clone();

            // Spawn each task without waiting for completion
            tokio::spawn(async move {
                let _ =
                    Self::process_user_data(metadata_clone, instruction_clone, user_bot_data_clone)
                        .await;
            });
        }

        Ok(())
    }
}

impl RaydiumV4Process {
    async fn process_user_data(
        metadata: carbon_core::instruction::InstructionMetadata,
        instruction: DecodedInstruction<RaydiumAmmV4Instruction>,
        user_bot_data: raydium_amm_monitor::backend::services::bot_service::UserBotData,
    ) -> CarbonResult<()> {
        let static_account_keys = metadata.transaction_metadata.message.static_account_keys();
        let writable_account_keys = &metadata.transaction_metadata.meta.loaded_addresses.writable;
        let readonly_account_keys = &metadata.transaction_metadata.meta.loaded_addresses.readonly;

        let mut account_keys: Vec<Pubkey> = vec![];

        account_keys.extend(static_account_keys);
        account_keys.extend(writable_account_keys);
        account_keys.extend(readonly_account_keys);

        let pool_id = &user_bot_data.bot_setting.pool_address;
        let user_id = &user_bot_data.user_id.to_string();

        let pool_address = match pool_id.parse::<Pubkey>() {
            Ok(pubkey) => pubkey,
            Err(_) => return Ok(()),
        };

        if !account_keys.contains(&pool_address) {
            return Ok(());
        }

        let initial_pool_info = raydium_amm_monitor::backend::services::bot_service::RealPoolInfo {
            user_bot_data: user_bot_data.clone(),
            pool_price: 0.0,
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

        let mut pool_info = initial_pool_info.clone();

        // Check if pool exists and get existing pool info
        let pool_exists = {
            let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
            real_pool_info.contains_key(pool_id)
        };

        if !pool_exists {
            println!("Initial pool info inserted");
            let mut real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
            real_pool_info.insert(pool_id.to_string(), vec![initial_pool_info.clone()]);
            drop(real_pool_info);
        } else {
            // Check if user already exists in this pool
            let user_exists = {
                let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                if let Some(pool_infos) = real_pool_info.get(pool_id) {
                    pool_infos
                        .iter()
                        .any(|info| info.user_bot_data.user_id.to_string() == user_id.clone())
                } else {
                    false
                }
            };

            if !user_exists {
                println!("Adding user to existing pool");
                let mut real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                if let Some(pool_infos) = real_pool_info.get_mut(pool_id) {
                    pool_infos.push(initial_pool_info.clone());
                }
                drop(real_pool_info);
            } else {
                println!("User already exists in pool, getting existing info");
                let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                if let Some(pool_infos) = real_pool_info.get(pool_id) {
                    for info in pool_infos {
                        if info.user_bot_data.user_id.to_string() == user_id.clone() {
                            println!("pool_id: {}", info.user_bot_data.bot_setting.pool_address);
                            pool_info = info.clone();
                            break;
                        }
                    }
                }
                drop(real_pool_info);
            }
        }

        println!("real_pool_info dropped");

        let instruction_clone: DecodedInstruction<RaydiumAmmV4Instruction> = instruction.clone();

        let _buy_ixs = match &instruction.data {
            RaydiumAmmV4Instruction::SwapBaseIn(_swap_base_in_data) => {
                if let Some(mut arranged) =
                    SwapBaseIn::arrange_accounts(&instruction_clone.accounts)
                {
                    let post_token_balance = metadata
                        .transaction_metadata
                        .meta
                        .post_token_balances
                        .clone();

                    let pre_token_balances = metadata
                        .transaction_metadata
                        .meta
                        .pre_token_balances
                        .clone();

                    let pre_token_balances_for_chain = pre_token_balances.clone();

                    let full_token_balances: Vec<_> = post_token_balance
                        .clone()
                        .into_iter()
                        .chain(pre_token_balances_for_chain.into_iter())
                        .collect();

                    let (coin_raw_info, pc_raw_info, pre_coin_raw_info, pre_pc_raw_info) =
                        get_coin_pc_mint(
                            post_token_balance.as_ref().unwrap_or(&vec![]),
                            pre_token_balances.as_ref().unwrap_or(&vec![]),
                            arranged.pool_coin_token_account,
                            arranged.pool_pc_token_account,
                            arranged.amm_authority,
                            &account_keys,
                        );

                    if let (Some(coin_info), Some(pc_info)) = (coin_raw_info, pc_raw_info) {
                        let user_coin_ata = get_associated_token_address(
                            &arranged.user_source_owner,
                            &Pubkey::from_str_const(&coin_info.1),
                        );
                        let user_coin1_ata = get_associated_token_address(
                            &arranged.user_source_owner,
                            &Pubkey::from_str_const(&pc_info.1),
                        );

                        let (input_mint, input_reserve, output_mint, output_reserve) =
                            if (user_coin_ata == arranged.user_source_token_account)
                                || (user_coin1_ata == arranged.user_destination_token_account)
                            {
                                (coin_info.1, coin_info.0, pc_info.1, pc_info.0)
                            } else {
                                (pc_info.1, pc_info.0, coin_info.1, coin_info.0)
                            };

                        let input_mint = Pubkey::from_str_const(&input_mint);
                        let output_mint = Pubkey::from_str_const(&output_mint);

                        let post_output_reserve_val = match output_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid output_reserve value: {}",
                                    output_reserve
                                );
                                return Ok(());
                            }
                        };
                        let post_input_reserve_val = match input_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!("Warning: Invalid input_reserve value: {}", input_reserve);
                                return Ok(());
                            }
                        };

                        // Get balance of base mint
                        let mint_decimal: u8;

                        if input_mint == WSOL {
                            mint_decimal = full_token_balances
                                .iter()
                                .flat_map(|balances| balances.iter())
                                .find(|balance| balance.mint == output_mint.to_string())
                                .and_then(|balance| Some(balance.ui_token_amount.decimals))
                                .unwrap_or(6);

                            let pool_price_sol = if post_output_reserve_val > 0.0 {
                                (post_input_reserve_val / 10f64.powf(9 as f64))
                                    / (post_output_reserve_val / 10f64.powf(mint_decimal as f64))
                            } else {
                                0.0 // Default to 0 if output reserve is zero
                            };
                            println!("signature : {}", metadata.transaction_metadata.signature);
                            println!("pool_price_sol 1: {:?}", pool_price_sol);
                            
                            {
                                let mut real_pool_info =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                for info in pool_info {
                                    if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                        info.latest_pool_price = pool_price_sol;
                                    }
                                }
                            }
                        } else {
                            mint_decimal = full_token_balances
                            .iter()
                            .flat_map(|balances| balances.iter())
                            .find(|balance| balance.mint == input_mint.to_string())
                            .and_then(|balance| Some(balance.ui_token_amount.decimals))
                            .unwrap_or(6);
                        
                        let pool_price_sol = if post_input_reserve_val > 0.0 {
                            (post_output_reserve_val / 10f64.powf(9 as f64))
                            / (post_input_reserve_val / 10f64.powf(mint_decimal as f64))
                        } else {
                            0.0 // Default to 0 if input reserve is zero
                        };
                        println!("signature : {}", metadata.transaction_metadata.signature);
                        println!("pool_price_sol 2: {:?}", pool_price_sol);

                            {
                                let mut real_pool_info =
                                    raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                for info in pool_info {
                                    if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                        info.latest_pool_price = pool_price_sol;
                                    }
                                }
                            }
                        }

                        arranged.user_source_owner = pool_info
                            .user_bot_data
                            .public_key
                            .parse::<Pubkey>()
                            .clone()
                            .unwrap();

                        let mut has_bought = false;
                        {
                            let real_pool_info =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                            let pool_info = real_pool_info.get(pool_id).unwrap();
                            for info in pool_info {
                                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                    has_bought = info.is_bought;
                                }
                            }
                        }

                        if !has_bought {
                            if input_mint == WSOL {
                                arranged.user_source_token_account = get_associated_token_address(
                                    &pool_info
                                        .user_bot_data
                                        .public_key
                                        .parse::<Pubkey>()
                                        .clone()
                                        .unwrap(),
                                    &input_mint,
                                );
                                arranged.user_destination_token_account =
                                    get_associated_token_address(
                                        &pool_info
                                            .user_bot_data
                                            .public_key
                                            .parse::<Pubkey>()
                                            .clone()
                                            .unwrap(),
                                        &output_mint,
                                    );
                            } else {
                                arranged.user_source_token_account = get_associated_token_address(
                                    &pool_info
                                        .user_bot_data
                                        .public_key
                                        .parse::<Pubkey>()
                                        .clone()
                                        .unwrap(),
                                    &output_mint,
                                );
                                arranged.user_destination_token_account =
                                    get_associated_token_address(
                                        &pool_info
                                            .user_bot_data
                                            .public_key
                                            .parse::<Pubkey>()
                                            .clone()
                                            .unwrap(),
                                        &input_mint,
                                    );
                            }
                        } else {
                            if input_mint == WSOL {
                                arranged.user_source_token_account = get_associated_token_address(
                                    &pool_info
                                        .user_bot_data
                                        .public_key
                                        .parse::<Pubkey>()
                                        .clone()
                                        .unwrap(),
                                    &output_mint,
                                );
                                arranged.user_destination_token_account =
                                    get_associated_token_address(
                                        &pool_info
                                            .user_bot_data
                                            .public_key
                                            .parse::<Pubkey>()
                                            .clone()
                                            .unwrap(),
                                        &input_mint,
                                    );
                            } else {
                                arranged.user_source_token_account = get_associated_token_address(
                                    &pool_info
                                        .user_bot_data
                                        .public_key
                                        .parse::<Pubkey>()
                                        .clone()
                                        .unwrap(),
                                    &input_mint,
                                );
                                arranged.user_destination_token_account =
                                    get_associated_token_address(
                                        &pool_info
                                            .user_bot_data
                                            .public_key
                                            .parse::<Pubkey>()
                                            .clone()
                                            .unwrap(),
                                        &output_mint,
                                    );
                            }
                        }

                        let entry_slippage = pool_info.user_bot_data.bot_setting.entry_slippage;
                        let exit_slippage = pool_info.user_bot_data.bot_setting.exit_slippage;
                        let buy_sol_amount =
                            pool_info.user_bot_data.bot_setting.buy_sol_amount.clone();

                        let amount_in = if !has_bought {
                            (buy_sol_amount * 10_f64.powf(9.0)) as u64
                        } else {
                            let token_balance = match RPC_CLIENT
                                .get_token_account_balance_with_commitment(
                                    &arranged.user_source_token_account,
                                    CommitmentConfig::processed(),
                                )
                                .await
                            {
                                Ok(response) => response.value.amount,
                                Err(_) => {
                                    return Ok(());
                                }
                            };

                            let token_amount = match token_balance.parse::<u64>() {
                                Ok(amount) => amount,
                                Err(_) => {
                                    return Ok(());
                                }
                            };

                            token_amount as u64
                        };

                        let output_reserve_val = match output_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid output_reserve value: {}",
                                    output_reserve
                                );
                                return Ok(());
                            }
                        };
                        let input_reserve_val = match input_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!("Warning: Invalid input_reserve value: {}", input_reserve);
                                return Ok(());
                            }
                        };

                        // Calculate amount_out by entry_slippage/exit slippage when buying
                        // Add safety check to prevent division by zero
                        let amount_out = if input_reserve_val + amount_in as f64 > 0.0 {
                            if has_bought {
                                0.997
                                    * (1.0 - exit_slippage / 100.0)
                                    * (amount_in as f64)
                                    * output_reserve_val
                                    / (input_reserve_val + amount_in as f64)
                            } else {
                                0.997
                                    * (1.0 - entry_slippage / 100.0)
                                    * (amount_in as f64)
                                    * output_reserve_val
                                    / (input_reserve_val + amount_in as f64)
                            }
                        } else {
                            println!("Warning: Invalid reserve values for amount_out calculation");
                            return Ok(());
                        };

                        let buy_exact_in_param = SwapBaseIn {
                            amount_in,
                            minimum_amount_out: amount_out as u64,
                        };

                        let mut ix: Vec<Instruction> = vec![];

                        let create_ata_ix =
                            arranged.get_create_idempotent_ata_ix(input_mint, output_mint);

                        ix.extend(create_ata_ix);

                        // Wrap SOL if buying WSOL
                        if !has_bought {
                            let wsol_ix = arranged.get_wrap_sol(
                                pool_info
                                    .user_bot_data
                                    .public_key
                                    .parse::<Pubkey>()
                                    .clone()
                                    .unwrap(),
                                buy_exact_in_param.clone(),
                            );
                            ix.extend(wsol_ix);
                        }

                        let swap_ix = arranged.get_swap_base_in_ix(buy_exact_in_param.clone());
                        ix.push(swap_ix.clone());

                        
                        // Wrap SOL if buying WSOL
                        if !has_bought {
                            let wsol_close = arranged.get_close_wsol(
                                pool_info
                                    .user_bot_data
                                    .public_key
                                    .parse::<Pubkey>()
                                    .clone()
                                    .unwrap()
                            );
                            ix.push(wsol_close.clone());
                        }

                        {
                            let mut real_pool_info =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                            let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                            for info in pool_info {
                                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                    info.swap_buy_ixs = ix.clone();
                                }
                            }
                        }
                    }
                }
            }

            _ => {
                println!("");
            }
        };

        let mut sent_signature = None;
        {
            let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
            let pool_info = real_pool_info.get(pool_id).unwrap();
            for info in pool_info {
                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                    sent_signature = info.signature.clone();
                }
            }
        }
        let metadata_signature = metadata.transaction_metadata.signature.to_string();
        let metadata_fee: u64 = metadata.transaction_metadata.meta.fee;
        let mut public_key = None;
        {
            let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
            let pool_info = real_pool_info.get(pool_id).unwrap();
            for info in pool_info {
                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                    public_key = info.user_bot_data.public_key.parse::<Pubkey>().ok();
                }
            }
        }
        let wsol_ata = get_associated_token_address(&public_key.unwrap(), &WSOL);
        let Some(idx) = account_keys.iter().position(|key| key == &wsol_ata) else {
            return Ok(());
        };
        // let wsol_lamports = metadata.transaction_metadata.meta.pre_balances[idx] - metadata.transaction_metadata.meta.post_balances[idx];
        if let Some(sig) = sent_signature {
            if sig == metadata_signature {
                let mut has_bought = false;
                {
                    let mut real_pool_info =
                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                    for info in pool_info {
                        if info.user_bot_data.user_id.to_string() == user_id.clone() {
                            has_bought = !info.is_bought;
                            info.is_bought = has_bought;
                        }
                    }
                }
                println!("Transaction signature confirmed: {}", sig);
                println!("IS_BOUGHT STATE: {}", has_bought);
                println!("Transaction fee: {}", metadata_fee);
                {
                    let mut real_pool_info =
                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                    for info in pool_info {
                        if info.user_bot_data.user_id.to_string() == user_id.clone() {
                            info.fee += metadata_fee as f64;
                        }
                    }
                }
                // println!("metadata: {:#?}", metadata);
                // Compute SOL deltas using signed math and convert lamports -> SOL
                let pre_lamports = metadata
                    .transaction_metadata
                    .meta
                    .pre_balances
                    .get(idx)
                    .copied()
                    .unwrap_or(0) as i128;
                let post_lamports = metadata
                    .transaction_metadata
                    .meta
                    .post_balances
                    .get(idx)
                    .copied()
                    .unwrap_or(0) as i128;

                // let input_lamports_delta: i128 = 0; // lamports spent (buy)
                // let output_lamports_delta: i128 = 0; // lamports received (sell)

                if has_bought {
                    // Just bought: SOL decreased
                    let input_lamports_delta = pre_lamports - post_lamports;
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_input_lamports_delta = Some(input_lamports_delta);
                            }
                        }
                    }
                    let input_sol = input_lamports_delta as f64 / 1_000_000_000.0;
                    println!("Input SOL: {}", input_sol);
                } else {
                    // Just sold: SOL increased
                    let output_lamports_delta = post_lamports - pre_lamports;
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_output_lamports_delta = Some(output_lamports_delta);
                            }
                        }
                    }
                    let output_sol = output_lamports_delta as f64 / 1_000_000_000.0;
                    println!("Output SOL: {}", output_sol);
                    let mut last_output_lamports_delta = None;
                    {
                        let real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                        let pool_info = real_pool_info.get(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                last_output_lamports_delta = info.last_output_lamports_delta;
                            }
                        }
                    }
                    let profit_sol =
                        (output_lamports_delta - last_output_lamports_delta.unwrap_or(0)) as f64
                            / 1_000_000_000.0;
                    println!("Profit: {}", profit_sol);
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_profit_sol = Some(profit_sol);
                            }
                        }
                    }
                    let mut last_input_lamports: Option<i128> = None;
                    {
                        let real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                        let pool_info = real_pool_info.get(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                last_input_lamports = info.last_input_lamports_delta;
                            }
                        }
                    }
                    let input_sol = last_input_lamports.unwrap_or(0) as f64 / 1_000_000_000.0;
                    let roi = if input_sol > 0.0 {
                        (profit_sol / input_sol) * 100.0
                    } else {
                        0.0
                    };
                    println!("ROI: {}", roi);
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_roi_pct = Some(roi);
                            }
                        }
                    }
                }

                if !has_bought {
                    let mut start_time: Option<std::time::Instant> = None;
                    {
                        let real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                        let pool_info = real_pool_info.get(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                start_time = info.start_time;
                            }
                        }
                    }
                    if let Some(start_time) = start_time {
                        let end_time = std::time::Instant::now();
                        println!("End time: {:?}", end_time);
                        let duration = end_time.duration_since(start_time);
                        println!("Time taken: {:?}", duration);

                        // Save duration to static variable
                        {
                            let mut real_pool_info =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                            let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                            for info in pool_info {
                                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                    info.last_duration = Some(duration);
                                    println!("✅ Saved duration to REAL_POOL_INFO: {:?}", duration);
                                }
                            }
                            drop(real_pool_info);
                        }
                    } else {
                        println!("No start time found");
                    }
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Processor for PumpSwapProcess {
    type InputType = InstructionProcessorInputType<PumpSwapInstruction>;

    async fn process(
        &mut self,
        (metadata, instruction, _nested_instructions, _instructions): Self::InputType,
        _metrics: Arc<MetricsCollection>,
    ) -> CarbonResult<()> {
        let user_list = raydium_amm_monitor::statics::USER_LIST.read().await;
        let user_list_clone = user_list.clone();
        drop(user_list); // Release the read lock immediately
        
        // Process all users concurrently without blocking
        for user_bot_data in user_list_clone.iter() {
            // Check if this user's pool_id matches the current transaction
            let pool_address = match user_bot_data.pool_id.parse::<Pubkey>() {
                Ok(pubkey) => pubkey,
                Err(_) => continue, // Skip invalid pool addresses
            };

            // Check if the pool is involved in this transaction
            let static_account_keys = metadata.transaction_metadata.message.static_account_keys();
            let writable_account_keys =
                &metadata.transaction_metadata.meta.loaded_addresses.writable;
            let readonly_account_keys =
                &metadata.transaction_metadata.meta.loaded_addresses.readonly;

            let mut account_keys: Vec<Pubkey> = vec![];
            account_keys.extend(static_account_keys);
            account_keys.extend(writable_account_keys);
            account_keys.extend(readonly_account_keys);

            if !account_keys.contains(&pool_address) || account_keys.contains(&RAY_V4_PROGRAM_ID) {
                continue; // Skip users whose pool is not involved in this transaction
            }

            let metadata_clone: carbon_core::instruction::InstructionMetadata = metadata.clone();
            let instruction_clone = instruction.clone();
            let user_bot_data_clone = user_bot_data.clone();

            // Spawn each task without waiting for completion
            tokio::spawn(async move {
                let _ =
                    Self::process_user_data(metadata_clone, instruction_clone, user_bot_data_clone)
                        .await;
            });
        }

        Ok(())
    }
}

impl PumpSwapProcess {
    async fn process_user_data(
        metadata: carbon_core::instruction::InstructionMetadata,
        instruction: DecodedInstruction<PumpSwapInstruction>,
        user_bot_data: raydium_amm_monitor::backend::services::bot_service::UserBotData,
    ) -> CarbonResult<()> {
        let static_account_keys = metadata.transaction_metadata.message.static_account_keys();
        let writable_account_keys = &metadata.transaction_metadata.meta.loaded_addresses.writable;
        let readonly_account_keys = &metadata.transaction_metadata.meta.loaded_addresses.readonly;
        let mut account_keys: Vec<Pubkey> = vec![];

        account_keys.extend(static_account_keys);
        account_keys.extend(writable_account_keys);
        account_keys.extend(readonly_account_keys);

        let pool_id = &user_bot_data.pool_id;
        let user_id = &user_bot_data.user_id.to_string();

        let initial_pool_info = raydium_amm_monitor::backend::services::bot_service::RealPoolInfo {
            user_bot_data: user_bot_data.clone(),
            pool_price: 0.0,
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

        let mut pool_info = initial_pool_info.clone();

        // Check if pool exists and get existing pool info
        let pool_exists = {
            let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
            real_pool_info.contains_key(pool_id)
        };

        if !pool_exists {
            println!("Initial pool info inserted");
            let mut real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
            real_pool_info.insert(pool_id.to_string(), vec![initial_pool_info.clone()]);
            drop(real_pool_info);
        } else {
            // Check if user already exists in this pool
            let user_exists = {
                let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                if let Some(pool_infos) = real_pool_info.get(pool_id) {
                    pool_infos
                        .iter()
                        .any(|info| info.user_bot_data.user_id.to_string() == user_id.clone())
                } else {
                    false
                }
            };

            if !user_exists {
                println!("Adding user to existing pool");
                let mut real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                if let Some(pool_infos) = real_pool_info.get_mut(pool_id) {
                    pool_infos.push(initial_pool_info.clone());
                }
                drop(real_pool_info);
            } else {
                println!("User already exists in pool, getting existing info");
                let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                if let Some(pool_infos) = real_pool_info.get(pool_id) {
                    for info in pool_infos {
                        if info.user_bot_data.user_id.to_string() == user_id.clone() {
                            println!("pool_id: {}", info.user_bot_data.bot_setting.pool_address);
                            pool_info = info.clone();
                            break;
                        }
                    }
                }
                drop(real_pool_info);
            }
        }

        println!("real_pool_info dropped");

        let instruction_clone: DecodedInstruction<PumpSwapInstruction> = instruction.clone();

        match &instruction.data {
            PumpSwapInstruction::Buy(_buy_params) => {
                if let Some(mut arranged) = Buy::arrange_accounts(&instruction_clone.accounts) {
                    println!("pumpswap arranged Buy");
                    let post_token_balance = metadata
                        .transaction_metadata
                        .meta
                        .post_token_balances
                        .clone();
                    let pre_token_balance = metadata
                        .transaction_metadata
                        .meta
                        .pre_token_balances
                        .clone();

                    let full_token_balances: Vec<_> = post_token_balance
                        .as_ref()
                        .into_iter()
                        .chain(pre_token_balance.as_ref().into_iter())
                        .collect();

                    let (base_raw_info, quote_raw_info, pre_base_raw_info, pre_quote_raw_info) =
                        get_coin_pc_mint(
                            post_token_balance.as_ref().unwrap_or(&vec![]),
                            pre_token_balance.as_ref().unwrap_or(&vec![]),
                            arranged.pool_base_token_account,
                            arranged.pool_quote_token_account,
                            arranged.pool,
                            &account_keys,
                        );

                    if let (
                        Some(base_info),
                        Some(quote_info),
                        Some(pre_base_info),
                        Some(pre_quote_info),
                    ) = (
                        base_raw_info,
                        quote_raw_info,
                        pre_base_raw_info,
                        pre_quote_raw_info,
                    ) {
                        let user_base_ata = get_associated_token_address(
                            &arranged.user,
                            &Pubkey::from_str_const(&base_info.1),
                        );
                        let user_quote_ata = get_associated_token_address(
                            &arranged.user,
                            &Pubkey::from_str_const(&quote_info.1),
                        );

                        let (
                            input_mint,
                            input_reserve,
                            output_mint,
                            output_reserve,
                            pre_input_reserve,
                            pre_output_reserve,
                        ) = if (user_base_ata == arranged.user_base_token_account)
                            || (user_quote_ata == arranged.user_quote_token_account)
                        {
                            (
                                base_info.1,
                                base_info.0,
                                quote_info.1,
                                quote_info.0,
                                pre_base_info.0,
                                pre_quote_info.0,
                            )
                        } else {
                            (
                                quote_info.1,
                                quote_info.0,
                                base_info.1,
                                base_info.0,
                                pre_quote_info.0,
                                pre_base_info.0,
                            )
                        };

                        let input_mint = Pubkey::from_str_const(&input_mint);
                        let output_mint = Pubkey::from_str_const(&output_mint);

                        // Get balance of base mint
                        let mint_decimal: u8;

                        let post_output_reserve_val = match output_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid output_reserve value: {}",
                                    output_reserve
                                );
                                return Ok(());
                            }
                        };

                        let post_input_reserve_val = match input_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!("Warning: Invalid input_reserve value: {}", input_reserve);
                                return Ok(());
                            }
                        };

                        let pre_output_reserve_val = match pre_output_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid pre_output_reserve value: {}",
                                    pre_output_reserve
                                );
                                return Ok(());
                            }
                        };

                        let pre_input_reserve_val = match pre_input_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid pre_input_reserve value: {}",
                                    pre_input_reserve
                                );
                                return Ok(());
                            }
                        };

                        if input_mint == WSOL {
                            mint_decimal = full_token_balances
                                .iter()
                                .flat_map(|balances| balances.iter())
                                .find(|balance| balance.mint == output_mint.to_string())
                                .and_then(|balance| Some(balance.ui_token_amount.decimals))
                                .unwrap_or(6);

                            // Calculate pool price
                            let pool_price_sol = if post_output_reserve_val > 0.0 {
                                (post_input_reserve_val / 10f64.powf(9 as f64))
                                    / (post_output_reserve_val / 10f64.powf(mint_decimal as f64))
                            } else {
                                0.0 // Default to 0 if output reserve is zero
                            };

                            println!("pool_price_sol: {:?}", pool_price_sol);

                            {
                                let mut real_pool_info =
                                    raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                for info in pool_info {
                                    if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                        info.latest_pool_price = pool_price_sol;
                                    }
                                }
                            }
                        } else {
                            mint_decimal = full_token_balances
                                .iter()
                                .flat_map(|balances| balances.iter())
                                .find(|balance| balance.mint == input_mint.to_string())
                                .and_then(|balance| Some(balance.ui_token_amount.decimals))
                                .unwrap_or(6);

                            let pool_price_sol = if post_input_reserve_val > 0.0 {
                                (post_output_reserve_val / 10f64.powf(9 as f64))
                                    / (post_input_reserve_val / 10f64.powf(mint_decimal as f64))
                            } else {
                                0.0 // Default to 0 if input reserve is zero
                            };

                            println!("pool_price_sol: {:?}", pool_price_sol);

                            {
                                let mut real_pool_info =
                                    raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                for info in pool_info {
                                    if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                        info.latest_pool_price = pool_price_sol;
                                    }
                                }
                            }
                        }

                        arranged.user = pool_info
                            .user_bot_data
                            .public_key
                            .parse::<Pubkey>()
                            .unwrap();

                        arranged.user_base_token_account = get_associated_token_address(
                            &pool_info
                                .user_bot_data
                                .public_key
                                .parse::<Pubkey>()
                                .unwrap(),
                            &arranged.base_mint,
                        );
                        arranged.user_quote_token_account = get_associated_token_address(
                            &pool_info
                                .user_bot_data
                                .public_key
                                .parse::<Pubkey>()
                                .unwrap(),
                            &arranged.quote_mint,
                        );

                        let mut has_bought = false;
                        {
                            let real_pool_info =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                            let pool_info = real_pool_info.get(pool_id).unwrap();
                            for info in pool_info {
                                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                    has_bought = info.is_bought;
                                }
                            }
                        }

                        let entry_slippage = pool_info.user_bot_data.bot_setting.entry_slippage;
                        let exit_slippage = pool_info.user_bot_data.bot_setting.exit_slippage;
                        let buy_sol_amount =
                            pool_info.user_bot_data.bot_setting.buy_sol_amount.clone();

                        let amount_in = if !has_bought {
                            (buy_sol_amount * 10_f64.powf(9.0)) as u64
                        } else {
                            let token_balance = match RPC_CLIENT
                                .get_token_account_balance_with_commitment(
                                    &arranged.user_base_token_account,
                                    CommitmentConfig::processed(),
                                )
                                .await
                            {
                                Ok(response) => response.value.amount,
                                Err(_) => {
                                    return Ok(());
                                }
                            };

                            let token_amount = match token_balance.parse::<u64>() {
                                Ok(amount) => amount,
                                Err(_) => {
                                    return Ok(());
                                }
                            };

                            token_amount
                        };

                        let pool_quote_token_reserves = match RPC_CLIENT
                            .get_token_account_balance_with_commitment(
                                &arranged.pool_quote_token_account,
                                CommitmentConfig::processed(),
                            )
                            .await
                        {
                            Ok(response) => response.value.amount,
                            Err(_) => {
                                return Ok(());
                            }
                        };

                        let pool_base_token_reserves = match RPC_CLIENT
                            .get_token_account_balance_with_commitment(
                                &arranged.pool_base_token_account,
                                CommitmentConfig::processed(),
                            )
                            .await
                        {
                            Ok(response) => response.value.amount,
                            Err(_) => {
                                return Ok(());
                            }
                        };

                        if !has_bought {
                            if arranged.quote_mint == WSOL {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                        "Warning: Invalid reserve values for amount_out calculation"
                                    );
                                        return Ok(());
                                    };

                                let wrap_buy_amount = amount_in as f64 * 1.1;

                                let mut instructions = vec![];

                                let create_ata_ix = arranged.get_create_idempotent_ata_ix();

                                let buy_ix = arranged.get_buy_ix(Buy {
                                    base_amount_out: required_token_amount,
                                    max_quote_amount_in: lamports_with_slippage,
                                });

                                instructions.extend(create_ata_ix);

                                if arranged.quote_mint == WSOL {
                                    let wrap_sol_ix = arranged.get_wrap_sol(wrap_buy_amount as u64);
                                    instructions.extend(wrap_sol_ix);
                                };

                                instructions.push(buy_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            } else {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                        "Warning: Invalid reserve values for amount_out calculation"
                                    );
                                        return Ok(());
                                    };

                                let wrap_buy_amount = amount_in as f64 * 1.1;

                                let mut instructions = vec![];

                                let create_ata_ix = arranged.get_create_idempotent_ata_ix();

                                let buy_ix = arranged.get_sell_ix(Sell {
                                    base_amount_in: required_token_amount,
                                    min_quote_amount_out: lamports_with_slippage,
                                });

                                instructions.extend(create_ata_ix);

                                if arranged.base_mint == WSOL {
                                    let wrap_sol_ix = arranged.get_wrap_sol(wrap_buy_amount as u64);

                                    instructions.extend(wrap_sol_ix);
                                };

                                instructions.push(buy_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            }
                        } else {
                            if arranged.quote_mint == WSOL {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                        "Warning: Invalid reserve values for amount_out calculation"
                                    );
                                        return Ok(());
                                    };

                                let mut instructions = vec![];

                                let sell_ix = arranged.get_sell_ix(Sell {
                                    base_amount_in: amount_in,
                                    min_quote_amount_out: lamports_with_slippage,
                                });

                                instructions.push(sell_ix);

                                let close_wsol_ix = arranged.get_close_wsol();
                                instructions.push(close_wsol_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            } else {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                        "Warning: Invalid reserve values for amount_out calculation"
                                    );
                                        return Ok(());
                                    };

                                let mut instructions = vec![];

                                let sell_ix = arranged.get_buy_ix(Buy {
                                    base_amount_out: amount_in,
                                    max_quote_amount_in: lamports_with_slippage,
                                });

                                instructions.push(sell_ix);
                                
                                let close_wsol_ix = arranged.get_close_wsol();
                                instructions.push(close_wsol_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        println!("Failed to arrange accounts");
                        return Ok(());
                    }
                }
            }
            PumpSwapInstruction::Sell(_sell_params) => {
                if let Some(mut arranged) = Sell::arrange_accounts(&instruction_clone.accounts) {
                    println!("pumpswap arranged Sell");
                    let post_token_balance = metadata
                        .transaction_metadata
                        .meta
                        .post_token_balances
                        .clone();
                    let pre_token_balance = metadata
                        .transaction_metadata
                        .meta
                        .pre_token_balances
                        .clone();

                    let full_token_balances: Vec<_> = post_token_balance
                        .as_ref()
                        .into_iter()
                        .chain(pre_token_balance.as_ref().into_iter())
                        .collect();

                    let (base_raw_info, quote_raw_info, pre_base_raw_info, pre_quote_raw_info) =
                        get_coin_pc_mint(
                            post_token_balance.as_ref().unwrap_or(&vec![]),
                            pre_token_balance.as_ref().unwrap_or(&vec![]),
                            arranged.pool_base_token_account,
                            arranged.pool_quote_token_account,
                            arranged.pool,
                            &account_keys,
                        );

                    if let (
                        Some(base_info),
                        Some(quote_info),
                        Some(pre_base_info),
                        Some(pre_quote_info),
                    ) = (
                        base_raw_info,
                        quote_raw_info,
                        pre_base_raw_info,
                        pre_quote_raw_info,
                    ) {
                        let user_base_ata = get_associated_token_address(
                            &arranged.user,
                            &Pubkey::from_str_const(&base_info.1),
                        );
                        let user_quote_ata = get_associated_token_address(
                            &arranged.user,
                            &Pubkey::from_str_const(&quote_info.1),
                        );

                        let (
                            input_mint,
                            input_reserve,
                            output_mint,
                            output_reserve,
                            pre_input_reserve,
                            pre_output_reserve,
                        ) = if (user_base_ata == arranged.user_base_token_account)
                            || (user_quote_ata == arranged.user_quote_token_account)
                        {
                            (
                                base_info.1,
                                base_info.0,
                                quote_info.1,
                                quote_info.0,
                                pre_base_info.0,
                                pre_quote_info.0,
                            )
                        } else {
                            (
                                quote_info.1,
                                quote_info.0,
                                base_info.1,
                                base_info.0,
                                pre_quote_info.0,
                                pre_base_info.0,
                            )
                        };

                        let input_mint = Pubkey::from_str_const(&input_mint);
                        let output_mint = Pubkey::from_str_const(&output_mint);

                        // Get balance of base mint
                        let mint_decimal: u8;

                        let post_output_reserve_val = match output_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid output_reserve value: {}",
                                    output_reserve
                                );
                                return Ok(());
                            }
                        };

                        let post_input_reserve_val = match input_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!("Warning: Invalid input_reserve value: {}", input_reserve);
                                return Ok(());
                            }
                        };

                        let pre_output_reserve_val = match pre_output_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid pre_output_reserve value: {}",
                                    pre_output_reserve
                                );
                                return Ok(());
                            }
                        };

                        let pre_input_reserve_val = match pre_input_reserve.parse::<f64>() {
                            Ok(val) => val,
                            Err(_) => {
                                println!(
                                    "Warning: Invalid pre_input_reserve value: {}",
                                    pre_input_reserve
                                );
                                return Ok(());
                            }
                        };

                        let input_change = post_input_reserve_val - pre_input_reserve_val;
                        let output_change = post_output_reserve_val - pre_output_reserve_val;

                        if input_mint == WSOL {
                            mint_decimal = full_token_balances
                                .iter()
                                .flat_map(|balances| balances.iter())
                                .find(|balance| balance.mint == output_mint.to_string())
                                .and_then(|balance| Some(balance.ui_token_amount.decimals))
                                .unwrap_or(6);

                            let base_mint_amount =
                                input_change as f64 / 10f64.powf(9 as f64);
                            let sell_amount = output_change as f64 / 10f64.powf(mint_decimal as f64);

                            let pool_price_sol = if post_output_reserve_val > 0.0 {
                                (post_input_reserve_val / 10f64.powf(9 as f64))
                                    / (post_output_reserve_val / 10f64.powf(mint_decimal as f64))
                            } else {
                                0.0 // Default to 0 if output reserve is zero
                            };

                            println!("pool_price_sol: {:?}", pool_price_sol);

                            println!(
                                "Base mint amount: {}, Sell amount: {}",
                                base_mint_amount, sell_amount
                            );

                            {
                                let mut real_pool_info =
                                    raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                for info in pool_info {
                                    if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                        info.latest_pool_price = pool_price_sol;
                                    }
                                }
                            }
                        } else {
                            mint_decimal = full_token_balances
                                .iter()
                                .flat_map(|balances| balances.iter())
                                .find(|balance| balance.mint == input_mint.to_string())
                                .and_then(|balance| Some(balance.ui_token_amount.decimals))
                                .unwrap_or(6);

                            let base_mint_amount =
                                input_change as f64 / 10f64.powf(mint_decimal as f64);
                            let sell_amount = output_change as f64 / 10f64.powf(9 as f64);

                            let pool_price_sol = if post_input_reserve_val > 0.0 {
                                (post_output_reserve_val / 10f64.powf(9 as f64))
                                    / (post_input_reserve_val / 10f64.powf(mint_decimal as f64))
                            } else {
                                0.0 // Default to 0 if input reserve is zero
                            };

                            println!("pool_price_sol: {:?}", pool_price_sol);

                            println!(
                                "Base mint amount: {}, Sell amount: {}",
                                base_mint_amount, sell_amount
                            );

                            {
                                let mut real_pool_info =
                                    raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                for info in pool_info {
                                    if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                        info.latest_pool_price = pool_price_sol;
                                    }
                                }
                            }
                        }

                        arranged.user = pool_info
                            .user_bot_data
                            .public_key
                            .parse::<Pubkey>()
                            .unwrap();

                        arranged.user_base_token_account = get_associated_token_address(
                            &pool_info
                                .user_bot_data
                                .public_key
                                .parse::<Pubkey>()
                                .unwrap(),
                            &arranged.base_mint,
                        );
                        arranged.user_quote_token_account = get_associated_token_address(
                            &pool_info
                                .user_bot_data
                                .public_key
                                .parse::<Pubkey>()
                                .unwrap(),
                            &arranged.quote_mint,
                        );

                        let mut has_bought = false;
                        {
                            let real_pool_info =
                                raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                            let pool_info = real_pool_info.get(pool_id).unwrap();
                            for info in pool_info {
                                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                    has_bought = info.is_bought;
                                }
                            }
                        }

                        let entry_slippage = pool_info.user_bot_data.bot_setting.entry_slippage;
                        let exit_slippage = pool_info.user_bot_data.bot_setting.exit_slippage;
                        let buy_sol_amount =
                            pool_info.user_bot_data.bot_setting.buy_sol_amount.clone();

                        let amount_in = if !has_bought {
                            (buy_sol_amount * 10_f64.powf(9.0)) as u64
                        } else {
                            let token_balance = match RPC_CLIENT
                                .get_token_account_balance_with_commitment(
                                    &arranged.user_base_token_account,
                                    CommitmentConfig::processed(),
                                )
                                .await
                            {
                                Ok(response) => response.value.amount,
                                Err(_) => {
                                    return Ok(());
                                }
                            };

                            let token_amount = match token_balance.parse::<u64>() {
                                Ok(amount) => amount,
                                Err(_) => {
                                    return Ok(());
                                }
                            };

                            token_amount
                        };

                        let pool_quote_token_reserves = match RPC_CLIENT
                            .get_token_account_balance_with_commitment(
                                &arranged.pool_quote_token_account,
                                CommitmentConfig::processed(),
                            )
                            .await
                        {
                            Ok(response) => response.value.amount,
                            Err(_) => {
                                return Ok(());
                            }
                        };

                        let pool_base_token_reserves = match RPC_CLIENT
                            .get_token_account_balance_with_commitment(
                                &arranged.pool_base_token_account,
                                CommitmentConfig::processed(),
                            )
                            .await
                        {
                            Ok(response) => response.value.amount,
                            Err(_) => {
                                return Ok(());
                            }
                        };

                        if !has_bought {
                            if arranged.quote_mint == WSOL {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                    "Warning: Invalid reserve values for amount_out calculation"
                                );
                                        return Ok(());
                                    };

                                let wrap_buy_amount = amount_in as f64 * 1.1;

                                let mut instructions = vec![];

                                let create_ata_ix = arranged.get_create_idempotent_ata_ix();

                                let buy_ix = arranged.get_buy_ix(Buy {
                                    base_amount_out: required_token_amount,
                                    max_quote_amount_in: lamports_with_slippage,
                                });

                                instructions.extend(create_ata_ix);

                                if arranged.quote_mint == WSOL {
                                    let wrap_sol_ix = arranged.get_wrap_sol(wrap_buy_amount as u64);
                                    instructions.extend(wrap_sol_ix);
                                };

                                instructions.push(buy_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            } else {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                    "Warning: Invalid reserve values for amount_out calculation"
                                );
                                        return Ok(());
                                    };

                                let wrap_buy_amount = amount_in as f64 * 1.1;

                                let create_ata_ix = arranged.get_create_idempotent_ata_ix();

                                let mut instructions = vec![];

                                let buy_ix = arranged.get_sell_ix(Sell {
                                    base_amount_in: required_token_amount,
                                    min_quote_amount_out: lamports_with_slippage,
                                });

                                instructions.extend(create_ata_ix);

                                if arranged.base_mint == WSOL {
                                    let wrap_sol_ix = arranged.get_wrap_sol(wrap_buy_amount as u64);

                                    instructions.extend(wrap_sol_ix);
                                };

                                instructions.push(buy_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            }
                        } else {
                            if arranged.quote_mint == WSOL {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 - entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                    "Warning: Invalid reserve values for amount_out calculation"
                                );
                                        return Ok(());
                                    };

                                let mut instructions = vec![];

                                let sell_ix = arranged.get_sell_ix(Sell {
                                    base_amount_in: amount_in,
                                    min_quote_amount_out: lamports_with_slippage,
                                });

                                instructions.push(sell_ix);

                                let close_wsol_ix = arranged.get_close_wsol();
                                instructions.push(close_wsol_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            } else {
                                let required_token_amount = sol_token_quote(
                                    amount_in,
                                    pool_quote_token_reserves.parse::<u64>().unwrap(),
                                    pool_base_token_reserves.parse::<u64>().unwrap(),
                                    true,
                                );

                                let lamports_with_slippage =
                                    if post_input_reserve_val + amount_in as f64 > 0.0 {
                                        if has_bought {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + exit_slippage))
                                                as u64
                                        } else {
                                            (required_token_amount as f64
                                                * 1.0025
                                                * (1.0 + entry_slippage))
                                                as u64
                                        }
                                    } else {
                                        println!(
                                    "Warning: Invalid reserve values for amount_out calculation"
                                );
                                        return Ok(());
                                    };

                                let sell_ix = arranged.get_buy_ix(Buy {
                                    base_amount_out: amount_in,
                                    max_quote_amount_in: lamports_with_slippage,
                                });

                                let mut instructions = vec![];

                                instructions.push(sell_ix);

                                let close_wsol_ix = arranged.get_close_wsol();
                                instructions.push(close_wsol_ix);

                                {
                                    let mut real_pool_info =
                                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                                    for info in pool_info {
                                        if info.user_bot_data.user_id.to_string() == user_id.clone()
                                        {
                                            info.swap_buy_ixs = instructions.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    return Ok(());
                }
            }
            _ => {
                // Handle other PumpSwapInstruction variants
                return Ok(());
            }
        };

        let mut sent_signature = None;
        {
            let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
            let pool_info = real_pool_info.get(pool_id).unwrap();
            for info in pool_info {
                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                    sent_signature = info.signature.clone();
                }
            }
        }
        let metadata_signature = metadata.transaction_metadata.signature.to_string();
        let metadata_fee: u64 = metadata.transaction_metadata.meta.fee;
        let mut public_key = None;
        {
            let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
            let pool_info = real_pool_info.get(pool_id).unwrap();
            for info in pool_info {
                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                    public_key = info.user_bot_data.public_key.parse::<Pubkey>().ok();
                }
            }
        }
        let wsol_ata = get_associated_token_address(&public_key.unwrap(), &WSOL);
        let Some(idx) = account_keys.iter().position(|key| key == &wsol_ata) else {
            return Ok(());
        };

        if let Some(sig) = sent_signature {
            if sig == metadata_signature {
                let mut has_bought = false;
                {
                    let mut real_pool_info =
                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                    for info in pool_info {
                        if info.user_bot_data.user_id.to_string() == user_id.clone() {
                            has_bought = !info.is_bought;
                            info.is_bought = has_bought;
                        }
                    }
                }
                println!("Transaction signature confirmed: {}", sig);
                println!("IS_BOUGHT STATE: {}", has_bought);
                println!("Transaction fee: {}", metadata_fee);
                {
                    let mut real_pool_info =
                        raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                    let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                    for info in pool_info {
                        if info.user_bot_data.user_id.to_string() == user_id.clone() {
                            info.fee += metadata_fee as f64;
                        }
                    }
                }
                // println!("metadata: {:#?}", metadata);
                // Compute SOL deltas using signed math and convert lamports -> SOL
                let pre_lamports = metadata
                    .transaction_metadata
                    .meta
                    .pre_balances
                    .get(idx)
                    .copied()
                    .unwrap_or(0) as i128;
                let post_lamports = metadata
                    .transaction_metadata
                    .meta
                    .post_balances
                    .get(idx)
                    .copied()
                    .unwrap_or(0) as i128;

                // let input_lamports_delta: i128 = 0; // lamports spent (buy)
                // let output_lamports_delta: i128 = 0; // lamports received (sell)

                if has_bought {
                    // Just bought: SOL decreased
                    let input_lamports_delta = pre_lamports - post_lamports;
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_input_lamports_delta = Some(input_lamports_delta);
                            }
                        }
                    }
                    let input_sol = input_lamports_delta as f64 / 1_000_000_000.0;
                    println!("Input SOL: {}", input_sol);
                } else {
                    // Just sold: SOL increased
                    let output_lamports_delta = post_lamports - pre_lamports;
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_output_lamports_delta = Some(output_lamports_delta);
                            }
                        }
                    }
                    let output_sol = output_lamports_delta as f64 / 1_000_000_000.0;
                    println!("Output SOL: {}", output_sol);
                    let mut last_output_lamports_delta = None;
                    {
                        let real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                        let pool_info = real_pool_info.get(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                last_output_lamports_delta = info.last_output_lamports_delta;
                            }
                        }
                    }
                    let profit_sol =
                        (output_lamports_delta - last_output_lamports_delta.unwrap_or(0)) as f64
                            / 1_000_000_000.0;
                    println!("Profit: {}", profit_sol);
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_profit_sol = Some(profit_sol);
                            }
                        }
                    }
                    let mut last_input_lamports: Option<i128> = None;
                    {
                        let real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                        let pool_info = real_pool_info.get(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                last_input_lamports = info.last_input_lamports_delta;
                            }
                        }
                    }
                    let input_sol = last_input_lamports.unwrap_or(0) as f64 / 1_000_000_000.0;
                    let roi = if input_sol > 0.0 {
                        (profit_sol / input_sol) * 100.0
                    } else {
                        0.0
                    };
                    println!("ROI: {}", roi);
                    {
                        let mut real_pool_info =
                            raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                        let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                        for info in pool_info {
                            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                                info.last_roi_pct = Some(roi);
                            }
                        }
                    }
                }

                // if !has_bought {
                //     let mut start_time: Option<std::time::Instant> = None;
                //     {
                //         let real_pool_info =
                //             raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
                //         let pool_info = real_pool_info.get(pool_id).unwrap();
                //         for info in pool_info {
                //             if info.user_bot_data.user_id.to_string() == user_id.clone() {
                //                 start_time = info.start_time;
                //             }
                //         }
                //     }
                //     if let Some(start_time) = start_time {
                //         let end_time = std::time::Instant::now();
                //         println!("End time: {:?}", end_time);
                //         let duration = end_time.duration_since(start_time);
                //         println!("Time taken: {:?}", duration);

                //         // Save duration to static variable
                //         {
                //             let mut real_pool_info =
                //                 raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
                //             let pool_info = real_pool_info.get_mut(pool_id).unwrap();
                //             for info in pool_info {
                //                 if info.user_bot_data.user_id.to_string() == user_id.clone() {
                //                     info.last_duration = Some(duration);
                //                     println!("✅ Saved duration to REAL_POOL_INFO: {:?}", duration);
                //                 }
                //             }
                //             drop(real_pool_info);
                //         }

                //         // Clean up bot state after selling
                //         cleanup_bot_after_sell(&pool_info).await;
                //     } else {
                //         println!("No start time found");
                //     }
                // }
            }
        }
        Ok(())
    }
}

/// Clean up bot state after selling tokens
async fn cleanup_bot_after_sell(
    pool_info: &raydium_amm_monitor::backend::services::bot_service::RealPoolInfo,
) {
    println!(
        "🧹 Cleaning up bot state after sell for user: {}",
        pool_info.user_bot_data.user_id
    );

    let pool_id = pool_info.user_bot_data.pool_id.clone();
    let user_id = pool_info.user_bot_data.user_id.clone();

    let mut start_time: Option<std::time::Instant> = None;
    {
        let real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.read().await;
        let pool_info = real_pool_info.get(&pool_id).unwrap();
        for info in pool_info {
            if info.user_bot_data.user_id.to_string() == user_id.clone() {
                start_time = info.start_time;
            }
        }
    }
    if let Some(start_time) = start_time {
        let end_time = std::time::Instant::now();
        println!("End time: {:?}", end_time);
        let duration = end_time.duration_since(start_time);
        println!("Time taken: {:?}", duration);

        // Save duration to static variable
        {
            let mut real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
            let pool_info = real_pool_info.get_mut(&pool_id).unwrap();
            for info in pool_info {
                if info.user_bot_data.user_id.to_string() == user_id.clone() {
                    info.last_duration = Some(duration);
                    println!("✅ Saved duration to REAL_POOL_INFO: {:?}", duration);
                }
            }
            drop(real_pool_info);
        }
    } else {
        println!("No start time found");
    }

    // Save metrics to MongoDB (best-effort)
    let profit_sol = pool_info.last_profit_sol.unwrap_or(0.0);
    let total_fees = pool_info.fee;
    let roi_pct = pool_info.last_roi_pct.unwrap_or(0.0);
    let duration_ms = pool_info
        .last_duration
        .as_ref()
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    println!(
        "📊 Trade metrics - Profit: {:.4} SOL, ROI: {:.2}%, Duration: {}ms",
        profit_sol, roi_pct, duration_ms
    );

    let _ = save_trade_metrics(
        pool_info.user_bot_data.user_id.to_string(),
        profit_sol,
        total_fees,
        roi_pct,
        duration_ms,
    )
    .await;

    // Remove from USER_LIST
    {
        let mut user_list = raydium_amm_monitor::statics::USER_LIST.write().await;
        let initial_count = user_list.len();
        user_list.retain(|user_bot_data| user_bot_data.user_id != pool_info.user_bot_data.user_id);
        let final_count = user_list.len();
        if initial_count != final_count {
            println!(
                "🧹 Removed {} entries from USER_LIST",
                initial_count - final_count
            );
        }
    }

    // Remove from REAL_POOL_INFO
    {
        let mut real_pool_info = raydium_amm_monitor::statics::REAL_POOL_INFO.write().await;
        let initial_pools = real_pool_info.len();
        real_pool_info.retain(|_pool_id, pool_infos| {
            let initial_users = pool_infos.len();
            pool_infos.retain(|current_pool_info| {
                current_pool_info.user_bot_data.user_id != pool_info.user_bot_data.user_id
            });
            let final_users = pool_infos.len();
            if initial_users != final_users {
                println!(
                    "🧹 Removed {} entries from pool",
                    initial_users - final_users
                );
            }
            !pool_infos.is_empty() // Keep the pool entry only if it has remaining users
        });
        let final_pools = real_pool_info.len();
        if initial_pools != final_pools {
            println!("🧹 Removed {} empty pools", initial_pools - final_pools);
        }
    }

    println!(
        "✅ Bot state cleanup completed for user: {}",
        pool_info.user_bot_data.user_id
    );
}
