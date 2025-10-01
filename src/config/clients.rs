use tokio::sync::OnceCell;
use std::env;

use crate::service::{Jito, Nozomi, ZeroSlot};

pub static NOZOMI_CLIENT: OnceCell<Nozomi> = OnceCell::const_new();
pub static ZSLOT_CLIENT: OnceCell<ZeroSlot> = OnceCell::const_new();
pub static JITO_CLIENT: OnceCell<Jito> = OnceCell::const_new();

pub async fn init_nozomi() {
    let _ = dotenv::dotenv().ok();

    let nozomi_api_key = env::var("NOZOMI_API_KEY").unwrap();

    let nozomi = Nozomi::new_auto(nozomi_api_key).await;
    nozomi.health_check(50);
    NOZOMI_CLIENT.set(nozomi).unwrap();
}

pub async fn init_zslot() {
    let _ = dotenv::dotenv().ok();

    let zslot_api_key = env::var("ZSLOT_API_KEY").unwrap();

    let zslot = ZeroSlot::new_auto(zslot_api_key).await;
    ZSLOT_CLIENT.set(zslot).unwrap();
}

pub async fn init_jito() {
    let _ = dotenv::dotenv().ok();
    
    let jito = Jito::new_auto(None).await;
    JITO_CLIENT.set(jito).unwrap();
}

