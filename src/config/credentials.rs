use once_cell::sync::Lazy;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use std::{env, sync::Arc};

pub static RPC_ENDPOINT: Lazy<String> = Lazy::new(|| {
    let _ = dotenv::dotenv().ok();
    
    let rpc_endpoint = env::var("RPC_ENDPOINT").unwrap();

    rpc_endpoint
});

pub static RPC_CLIENT: Lazy<Arc<RpcClient>> = Lazy::new(|| {
    let _ = dotenv::dotenv().ok();
    
    let rpc_endpoint = env::var("RPC_ENDPOINT").unwrap();

    Arc::new(RpcClient::new_with_commitment(
        rpc_endpoint,
        CommitmentConfig::processed(),
    ))
});
