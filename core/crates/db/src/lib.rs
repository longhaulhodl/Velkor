pub use sqlx::postgres::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::info;

/// Create a PostgreSQL connection pool from a database URL.
pub async fn create_pg_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(database_url)
        .await?;

    info!("PostgreSQL connection pool established");
    Ok(pool)
}
