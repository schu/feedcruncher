use anyhow::Result;
use sqlx::sqlite::SqlitePool;
use sqlx::{Pool, Sqlite};

pub async fn get_db_pool(db_path: &str) -> Result<Pool<Sqlite>> {
    // Set the Sqlite mode to read-write-create
    // to create the database if it doesn't exist
    // https://www.sqlite.org/c3ref/open.html
    let db_path = format!("{}?mode=rwc", db_path);

    let pool = SqlitePool::connect(&db_path).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
