use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub async fn start_backend_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load configuration
    let config = match crate::backend::config::Config::from_env() {
        Ok(config) => {
            println!("Configuration loaded successfully");
            config
        },
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            return Err(e.into());
        }
    };

    // Initialize tracing (only if not already initialized)
    let _ = tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&config.rust_log)),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    println!("Tracing initialized");

    // Initialize database connection
    let db = match crate::backend::db::connection::init_database(&config).await {
        Ok(db) => {
            println!("Database connection established");
            db
        },
        Err(e) => {
            eprintln!("Failed to connect to database: {}", e);
            return Err(e.into());
        }
    };

    // Create application
    let app = crate::backend::app::create_app(db);
    println!("Application created");

    // Run the application
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    println!("Starting server on {}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            println!("TCP listener bound successfully");
            listener
        },
        Err(e) => {
            eprintln!("Failed to bind TCP listener: {}", e);
            return Err(e.into());
        }
    };

    println!("Server is running on http://{}", addr);
    
    match axum::serve(listener, app).await {
        Ok(_) => {
            println!("Server stopped gracefully");
            Ok(())
        },
        Err(e) => {
            eprintln!("Server error: {}", e);
            Err(Box::new(e))
        }
    }
}
