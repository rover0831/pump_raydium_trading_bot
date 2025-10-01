use anyhow::{Context, Result};
use mongodb::{bson::doc, options::IndexOptions, Client, Database, IndexModel};

use crate::backend::config::Config;

pub type AppDatabase = Database;

pub async fn init_database(config: &Config) -> Result<AppDatabase> {
    let client = Client::with_uri_str(config.mongodb_connection_string())
        .await
        .context("Failed to connect to MongoDB")?;

    // Use database from URI if present, otherwise fallback
    let db_name = client
        .default_database()
        .map(|db| db.name().to_string())
        .unwrap_or_else(|| "trading".to_string());

    let database = client.database(&db_name);

    // Create indexes for collections
    create_indexes(&database)
        .await
        .context("Failed to create MongoDB indexes")?;

    Ok(database)
}

async fn create_indexes(database: &AppDatabase) -> Result<()> {
    let users = database.collection::<crate::backend::models::user::User>("users");
    let bots = database.collection::<crate::backend::models::bot::BotSettings>("bot_settings");

    // User indexes
    let user_indexes = vec![
        IndexModel::builder()
            .keys(doc! { "email": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
        IndexModel::builder()
            .keys(doc! { "username": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    ];

    // Bot indexes
    let bot_indexes = vec![
        IndexModel::builder()
            .keys(doc! { "user_id": 1 })
            .build(),
        IndexModel::builder()
            .keys(doc! { "user_id": 1, "name": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build(),
    ];

    // Create user indexes
    for index in user_indexes {
        users
            .create_index(index)
            .await
            .context("Failed to create index on users collection")?;
    }

    // Create bot indexes
    for index in bot_indexes {
        bots
            .create_index(index)
            .await
            .context("Failed to create index on bot_settings collection")?;
    }

    println!("✅ Indexes ensured on 'users' and 'bot_settings' collections");

    Ok(())
}
