#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(unused)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

mod config;
mod database;
mod feed;
mod notification;
mod webhook;

mod prelude {
    pub use crate::config::*;
    pub use crate::database::*;
    pub use crate::feed::*;
    pub use crate::notification::*;
    pub use crate::webhook::*;
}

use crate::prelude::*;

use std::process::exit;
use std::sync::Arc;
use std::vec;

use anyhow::{anyhow, Result};
use clap::Parser;
use sqlx::{Pool, Sqlite};
use tokio::sync::Mutex;
use tokio::task::JoinSet;

#[derive(Parser, Debug)]
#[clap(version = "0.1.0")]
struct Opts {
    #[clap(short, long)]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts: Opts = Opts::parse();

    let config: Config = match read_config_file(opts.config).await {
        Ok(s) => s,
        Err(e) => {
            println!("failed to read config: {}", e);
            exit(1);
        }
    };

    println!("config: {:#?}", config);

    let poll = config.poll.unwrap_or(true);

    let poll_sleep_dur = if let Some(d) = config.poll_sleep_dur {
        tokio::time::Duration::from_secs(d)
    } else {
        tokio::time::Duration::from_secs(600)
    };

    let db_path = if let Some(p) = config.db_path {
        p
    } else {
        "sqlite://./feedcruncher.sqlite3".to_string()
    };

    let db = get_db_pool(&db_path).await?;

    // Create list of feed objects
    let feeds = config.feeds.iter().map(|f| {
        let webhook_urls = if let Some(v) = f.webhook_urls.clone() {
            Some(v.clone())
        } else {
            config.webhook_urls.clone()
        };

        match feed_from_config(&f.kind, &f.url, webhook_urls, &db.clone()) {
            Ok(feed) => feed,
            Err(err) => {
                println!("{}", err);
                exit(1);
            }
        }
    });

    // Save feeds
    let mut set = JoinSet::new();
    for feed in feeds.clone() {
        set.spawn(async move { feed.lock().await.save().await });
    }
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => {
                println!("failed to save feed: {}", e);
                exit(1);
            }
            Err(e) => {
                println!("failed to join saving feeds: {}", e);
                exit(1);
            }
        }
    }
    assert!(set.is_empty());

    loop {
        // Fetch feeds
        let mut set = JoinSet::new();
        for feed in feeds.clone() {
            let feed = feed.clone();
            set.spawn(async move { feed.lock().await.fetch().await });
        }

        // Collect feed items from feeds
        let mut items: Vec<FeedItem> = vec![];
        while let Some(res) = set.join_next().await {
            let fetched_items = match res {
                Ok(Ok(items)) => items,
                Ok(Err(e)) => {
                    println!("failed to fetch feed: {}", e);
                    continue;
                }
                Err(e) => {
                    println!("failed to join fetching feeds: {}", e);
                    exit(1);
                }
            };
            items.extend(fetched_items);
        }
        assert!(set.is_empty());

        // Save feed items, webhooks and notifications
        let mut set = JoinSet::new();
        for item in items {
            let db = db.clone();

            set.spawn(async move {
                // Save feed item
                match item.save().await {
                    Ok(_) => (),
                    Err(e) => {
                        return Err(anyhow!("failed to save item '{}': {}", item.guid, e));
                    }
                };

                // Get the webhooks for the feed
                let webhooks: Vec<Box<dyn Webhook>> =
                    if let Some(v) = item.feed.lock().await.webhook_urls() {
                        v.iter()
                            .filter_map(|url| match webhook_from_url(url.clone()) {
                                Ok(h) => Some(h),
                                Err(e) => {
                                    println!("{}", e);
                                    None
                                }
                            })
                            .collect()
                    } else {
                        return Err(anyhow!(
                            "got no webhook urls for feed '{}'",
                            item.feed.lock().await.url()
                        ));
                    };

                // Save webhooks and notifications
                for webhook in webhooks {
                    let webhook_id = match webhook.save(&item.db).await {
                        Ok(id) => id,
                        Err(e) => {
                            return Err(anyhow!(
                                "failed to save webhook '{}': {}",
                                webhook.url(),
                                e
                            ));
                        }
                    };

                    let item_id = match item.id().await {
                        Ok(id) => id,
                        Err(e) => {
                            return Err(anyhow!("failed to get item id '{}': {}", item.guid, e));
                        }
                    };

                    let notification = Notification {
                        feed_item_id: item_id,
                        webhook_id,
                    };

                    // We don't want to send notifications for items
                    // if a feed was just added ...
                    let sent = match item.feed.lock().await.is_new().await {
                        Ok(is_new) => is_new,
                        Err(e) => {
                            return Err(anyhow!(
                                "failed to check if feed is new '{}': {}",
                                item.feed.lock().await.url(),
                                e
                            ));
                        }
                    };

                    // ... so we save notifications as sent if this is
                    // the first time we're fetching the feed
                    match notification.save(&db.clone(), sent).await {
                        Ok(_) => (),
                        Err(e) => {
                            return Err(anyhow!(
                                "failed to save notification '{}': {}",
                                item.guid,
                                e
                            ));
                        }
                    };
                }

                Ok(())
            });
        }
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(_)) => (),
                Ok(Err(e)) => {
                    println!("failed to save item: {}", e);
                    // This could happen if the feed item has a duplicate guid;
                    // we don't want to exit in this case and ignore this for now
                }
                Err(e) => {
                    println!("failed to join saving items: {}", e);
                    exit(1);
                }
            }
        }
        assert!(set.is_empty());

        // Set `feeds.is_new` to false now that we've fetched the feeds
        for feed in feeds.clone() {
            set.spawn(async move { feed.lock().await.set_is_new(false).await });
        }
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(_)) => (),
                Ok(Err(e)) => {
                    println!("failed to set `feed.is_new` to false: {}", e);
                }
                Err(e) => {
                    println!("failed to join setting `feed.is_new`: {}", e);
                    exit(1);
                }
            }
        }
        assert!(set.is_empty());

        // Finally, send pending notifications

        let noficiations = Notifications::get_unsent(&db).await?;

        for notification in noficiations {
            let db = db.clone();

            set.spawn(async move { notification.send(&db).await });
        }
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(_)) => (),
                Ok(Err(e)) => {
                    println!("failed to send notification: {}", e);
                }
                Err(e) => {
                    println!("failed to join sending notifications: {}", e);
                    exit(1);
                }
            }
        }
        assert!(set.is_empty());

        if !poll {
            break;
        }
        tokio::time::sleep(poll_sleep_dur).await;
    }

    Ok(())
}

fn feed_from_config(
    kind: &str,
    url: &str,
    webhook_urls: Option<Vec<String>>,
    db: &Pool<Sqlite>,
) -> Result<Arc<Mutex<Box<dyn Feed>>>> {
    match kind {
        "atom" => Ok(Arc::new(Mutex::new(Box::new(AtomFeed::new(
            url.to_string(),
            webhook_urls,
            db,
        )?)))),
        "rss" => Ok(Arc::new(Mutex::new(Box::new(RSSFeed::new(
            url.to_string(),
            webhook_urls,
            db,
        )?)))),
        _ => Err(anyhow!("unknown feed kind '{}' for feed '{}'", kind, url)),
    }
}
