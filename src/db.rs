//! Database connection pool initialisation.

use sqlx::{PgPool, postgres::PgPoolOptions};

/// Creates a Postgres connection pool from the `DATABASE_URL` environment variable.
///
/// Panics if `DATABASE_URL` is not set or if the connection cannot be established.
pub async fn create_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in .env or environment");

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database")
}
