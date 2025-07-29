use sqlx::Pool;
use sqlx::postgres::PgPoolOptions;

pub struct Database {
    pub pool: Pool<sqlx::Postgres>,
}

impl Database {
    pub async fn new_pool(url: &str) -> Pool<sqlx::Postgres> {
        PgPoolOptions::new()
            .max_connections(10)
            .min_connections(5)
            .idle_timeout(std::time::Duration::from_secs(30))
            .connect(url)
            .await
            .expect("Failed to create DB pool")
    }
}
