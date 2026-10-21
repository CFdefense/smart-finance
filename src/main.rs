//! Smart Finance API — application entry point.

use smart_finance::log;
use smart_finance::plaid::client::PlaidClient;
use std::{env, net::SocketAddr, str::FromStr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load ENV variables
    dotenvy::dotenv().ok();

    // Intialize logging
    log::init_panic_handler();
    log::init_logger();

    // Read binding addresses
    let frontend_url = env::var("FRONTEND_URL").expect("FRONTEND_URL must be set");
    let bind_address = env::var("BIND_ADDRESS").expect("BIND_ADDRESS must be set");

    // Initialize the app router
    let app = smart_finance::create_router(&frontend_url, PlaidClient::new()).await;

    // Create a socket address
    let addr = SocketAddr::from_str(&bind_address).expect("Invalid BIND_ADDRESS format");
    tracing::info!("Server starting on {bind_address}");

    // Bind the address to the TCP listener
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Serve the app
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
