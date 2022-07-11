pub mod models;
pub mod schema;

#[macro_use]
extern crate diesel;
extern crate dotenv;

use std::env;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2::*;
use dotenv::dotenv;

pub fn create_db_conn_pool() -> Arc<Pool<ConnectionManager<SqliteConnection>>> {
    dotenv().ok();

    let database_url = match env::var("DATABASE_URL") {
        Ok(u) => u,
        Err(_) => {
            println!("DATABASE_URL not in env - using default");
            "feedcruncher.sqlite3".to_string()
        }
    };

    let manager = ConnectionManager::<SqliteConnection>::new(&database_url);
    let pool = Arc::new(Pool::builder().max_size(2).build(manager).unwrap());

    let mut db_conn = pool.get().unwrap();

    // Enable sqlite foreign key support
    diesel::sql_query("PRAGMA foreign_keys = on")
        .execute(&mut db_conn)
        .unwrap();
    drop(db_conn);

    pool
}
