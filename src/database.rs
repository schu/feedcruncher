use std::env;

use anyhow::Result;
use dotenv::dotenv;
use sqlx::sqlite::SqlitePool;
use sqlx::{Pool, Sqlite};

const DEFAULT_DB_URL: &str = "feedcruncher.sqlite3";

pub async fn get_db_pool() -> Result<Pool<Sqlite>> {
    dotenv().ok();

    let mut database_url = match env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            println!(
                "DATABASE_URL not in env - using default ('{}')",
                DEFAULT_DB_URL
            );
            DEFAULT_DB_URL.to_string()
        }
    };

    // Set the Sqlite mode to read-write-create
    // to create the database if it doesn't exist
    // https://www.sqlite.org/c3ref/open.html
    database_url += "?mode=rwc";

    let pool = SqlitePool::connect(&database_url).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}
