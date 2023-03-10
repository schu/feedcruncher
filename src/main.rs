#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(unused)]
#![warn(rust_2018_idioms)]

use core::fmt::Debug;

use std::fs::File;
use std::io::prelude::*;
use std::process::exit;
use std::sync::mpsc;
use std::thread;
use std::time;

use anyhow::{anyhow, Result};
use chrono::prelude::*;
use clap::Parser;
use diesel::prelude::*;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use reqwest::StatusCode;

use serde::{Deserialize, Serialize};

use feedcruncher::models::{
    Item, NewFeed, NewItem, NewNotification, NewWebhook, Notification, Webhook,
};
use feedcruncher::schema::{feeds, items, notifications, webhooks};
use feedcruncher::*;

#[derive(Parser, Debug)]
#[clap(version = "0.1.0")]
struct Opts {
    #[clap(short, long)]
    config: String,
}

const DB_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

fn main() {
    let db_conn_pool = create_db_conn_pool();
    let db_conn = &mut db_conn_pool.get().unwrap();
    let opts: Opts = Opts::parse();
    let (tx, rx): (mpsc::Sender<FeedItem>, mpsc::Receiver<FeedItem>) = mpsc::channel();

    match db_conn.run_pending_migrations(DB_MIGRATIONS) {
        Ok(_) => (),
        Err(e) => {
            println!("failed to run database migrations: {}", e);
            exit(1);
        }
    }

    let config: Config = match read_config_file(opts.config) {
        Ok(s) => s,
        Err(e) => {
            println!("failed to read config: {}", e);
            exit(1);
        }
    };

    println!("Watching {:#?}", config.feeds);

    let sleep_dur = if let Some(d) = config.sleep_dur {
        time::Duration::from_secs(d)
    } else {
        time::Duration::from_secs(600)
    };

    for feed in config.feeds {
        let feed = FeedReqwest::new(feed);
        let tx = tx.clone();
        thread::spawn(move || poll(&feed, tx, sleep_dur));
    }

    thread::spawn(move || dispatch(&mut db_conn_pool.get().unwrap()));

    loop {
        println!("Waiting for new feed items ...");
        let received = match rx.recv() {
            Ok(received) => received,
            Err(e) => {
                println!("failed to receive message: {}", e);
                continue;
            }
        };
        println!("Received new item ...");
        println!("{:#?}", received.item);

        let webhooks = if let Some(ref url) = received.config.webhooks {
            url.clone()
        } else if let Some(ref url) = config.webhooks {
            url.clone()
        } else {
            println!("got no webhook_url for feed {}", received.config.url);
            println!("cannot process feed item");
            continue;
        };

        let now = Utc::now().to_string();
        let new_feed = NewFeed {
            url: &received.config.url,
            last_fetched_at: &now,
        };

        diesel::insert_into(feeds::table)
            .values(&new_feed)
            .on_conflict(feeds::url)
            .do_update()
            .set(feeds::last_fetched_at.eq(&now))
            .execute(db_conn)
            .unwrap();

        let feed_id = feeds::table
            .select(feeds::id)
            .filter(feeds::url.eq(&received.config.url))
            .first(db_conn)
            .unwrap();

        let guid = received.item.guid().unwrap().value().to_string();
        let new_item = NewItem {
            feed: &feed_id,
            guid: &guid,
            link: received.item.link().unwrap(),
            title: if let Some(title) = received.item.title() {
                title
            } else {
                ""
            },
            fetched_at: &now,
        };

        diesel::insert_into(items::table)
            .values(&new_item)
            .on_conflict(items::guid)
            .do_nothing()
            .execute(db_conn)
            .unwrap();

        let item_id: i32 = items::table
            .select(items::id)
            .filter(items::guid.eq(&guid))
            .first(db_conn)
            .unwrap();

        for webhook_url in webhooks.iter() {
            let new_webhook = NewWebhook { url: webhook_url };

            diesel::insert_into(webhooks::table)
                .values(&new_webhook)
                .on_conflict(webhooks::url)
                .do_nothing()
                .execute(db_conn)
                .unwrap();

            let webhook_id: i32 = webhooks::table
                .select(webhooks::id)
                .filter(webhooks::url.eq(&webhook_url))
                .first(db_conn)
                .unwrap();

            if notifications::table
                .filter(
                    notifications::item
                        .eq(item_id)
                        .and(notifications::webhook.eq(webhook_id)),
                )
                .first::<Notification>(db_conn)
                .is_ok()
            {
                // Notification already stored, nothing to do
                continue;
            }

            let new_notification = NewNotification {
                item: &item_id,
                webhook: &webhook_id,
                sent: &0,
                sent_at: "",
            };

            diesel::insert_into(notifications::table)
                .values(&new_notification)
                .execute(db_conn)
                .unwrap();
        }
    }
}

fn hook_from_url(url: String) -> Result<Box<dyn Hook>> {
    if url == "-" {
        return Ok(Box::new(WebhookNoop::new()));
    }
    if url.contains("https://discordapp.com/api") || url.contains("https://discord.com/api") {
        return Ok(Box::new(WebhookDiscord::new(url)));
    }
    if url.contains("https://hooks.slack.com") {
        return Ok(Box::new(WebhookSlack::new(url)));
    }
    Err(anyhow!("unknown webhook target: '{}'", url))
}

fn read_config_file(path: String) -> Result<Config> {
    let mut config_file = File::open(path)?;
    let mut config_string = String::new();
    config_file.read_to_string(&mut config_string)?;
    Ok(toml::from_str(&config_string)?)
}

#[derive(Debug, Deserialize)]
struct Config {
    feeds: Vec<FeedConfig>,
    sleep_dur: Option<u64>,
    webhooks: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct FeedConfig {
    url: String,
    webhooks: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
struct FeedItem {
    config: FeedConfig,
    item: rss::Item,
}

trait RSSFeed {
    fn get_config(&self) -> FeedConfig;
    fn get_items(&self) -> Result<Vec<rss::Item>>;
}

struct FeedReqwest {
    config: FeedConfig,
}

impl FeedReqwest {
    fn new(config: FeedConfig) -> FeedReqwest {
        FeedReqwest { config }
    }
}

impl RSSFeed for FeedReqwest {
    fn get_config(&self) -> FeedConfig {
        self.config.clone()
    }

    fn get_items(&self) -> Result<Vec<rss::Item>> {
        let res = reqwest::blocking::get(&self.config.url)?;
        let feed_xml = res.text()?;
        Ok(rss::Channel::read_from(feed_xml.as_bytes())?.into_items())
    }
}

trait Hook {
    fn push(&self, item: &Item) -> Result<()>;
}

impl Debug for dyn Hook {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "dyn Hook")
    }
}

fn push_message(url: &str, msg: String) -> Result<()> {
    let response = reqwest::blocking::Client::new()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(msg)
        .send()?;
    let status = response.status();
    match status {
        StatusCode::OK | StatusCode::NO_CONTENT => Ok(()),
        _ => Err(anyhow!("http error: '{}'", status)),
    }
}

struct WebhookDiscord {
    url: String,
}

#[derive(Serialize)]
struct DiscordMessage {
    content: String,
}

impl WebhookDiscord {
    fn new(url: String) -> WebhookDiscord {
        WebhookDiscord { url }
    }

    fn render_message(&self, item: &Item) -> Result<String> {
        let msg_title = match &item.title {
            Some(t) => format!("{}\n", t),
            None => "".to_string(),
        };
        let msg_content = format!("{}{}", msg_title, item.guid);
        let msg = DiscordMessage {
            content: msg_content,
        };
        Ok(serde_json::to_string(&msg)?)
    }
}

impl Hook for WebhookDiscord {
    fn push(&self, item: &Item) -> Result<()> {
        push_message(&self.url, self.render_message(item)?)
    }
}

struct WebhookSlack {
    url: String,
}

#[derive(Serialize)]
struct SlackMessage {
    text: String,
}

impl WebhookSlack {
    fn new(url: String) -> WebhookSlack {
        WebhookSlack { url }
    }

    fn render_message(&self, item: &Item) -> Result<String> {
        let msg_title = match &item.title {
            Some(t) => format!("{}\n", t),
            None => "".to_string(),
        };
        let msg_content = format!("{}{}", msg_title, item.guid);
        let msg = SlackMessage { text: msg_content };
        Ok(serde_json::to_string(&msg)?)
    }
}

impl Hook for WebhookSlack {
    fn push(&self, item: &Item) -> Result<()> {
        push_message(&self.url, self.render_message(item)?)
    }
}

struct WebhookNoop {}

impl WebhookNoop {
    fn new() -> WebhookNoop {
        WebhookNoop {}
    }
}

impl Hook for WebhookNoop {
    fn push(&self, _item: &Item) -> Result<()> {
        Ok(())
    }
}

fn dispatch(db_conn: &mut SqliteConnection) {
    loop {
        let unsent_notifications = notifications::table
            .filter(notifications::sent.eq(0))
            .load::<Notification>(db_conn)
            .unwrap();

        if !unsent_notifications.is_empty() {
            println!("Dispatching notifications ...");
        }

        for unsent in unsent_notifications {
            println!("{:#?}", unsent);

            let hook: Webhook = webhooks::table.find(unsent.webhook).first(db_conn).unwrap();

            let hook = hook_from_url(hook.url).unwrap();

            let item: Item = items::table.find(unsent.item).first(db_conn).unwrap();

            println!("item: {:#?}", item);

            match hook.push(&item) {
                Ok(_) => (),
                Err(e) => {
                    println!("failed to push message: {}", e);
                    continue;
                }
            };

            diesel::update(notifications::table.find(unsent.id))
                .set((
                    notifications::sent.eq(1),
                    notifications::sent_at.eq(Utc::now().to_string()),
                ))
                .execute(db_conn)
                .unwrap();
        }

        thread::sleep(time::Duration::from_secs(60));
    }
}

fn guids_from_items(items: &[rss::Item]) -> Vec<String> {
    items
        .iter()
        .filter(|item| item.guid().is_some())
        .map(|item| match item.guid() {
            Some(guid) => guid.value().to_string(),
            None => panic!("cannot happen"),
        })
        .collect()
}

fn poll(feed: &impl RSSFeed, tx: mpsc::Sender<FeedItem>, sleep_dur: time::Duration) {
    let feed_items = feed.get_items().unwrap();
    let mut feed_guids = guids_from_items(&feed_items);

    loop {
        thread::sleep(sleep_dur);

        let feed_items: Vec<rss::Item> = match feed.get_items() {
            Ok(items) => items,
            Err(e) => {
                println!("failed to get feed: {}", e);
                continue;
            }
        };

        // Filter out items with known, no or empty guid
        let new_items: Vec<rss::Item> = feed_items
            .into_iter()
            .filter(|item| match item.guid() {
                Some(guid) => {
                    let guid_val = guid.value().to_string();
                    !(feed_guids.contains(&guid_val) || guid_val.is_empty())
                }
                None => false,
            })
            .collect();

        // Send new items to receiver thread
        for item in new_items {
            // Items without guid were filtered out above, i.e. safe to unwrap
            let guid = item.guid().unwrap().value().to_string();
            let feed_item = FeedItem {
                config: feed.get_config(),
                item,
            };
            match tx.send(feed_item) {
                Ok(_) => {
                    feed_guids.push(guid);
                }
                Err(e) => {
                    println!("failed to send message to receiver thread: {}", e);
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    fn create_test_item(guid_value: String) -> rss::Item {
        let mut item = rss::Item::default();
        let mut guid = rss::Guid::default();
        guid.set_value(guid_value);
        item.set_guid(guid);
        item
    }

    fn default_test_items() -> Vec<rss::Item> {
        let mut items: Vec<rss::Item> = Vec::new();
        items.push(create_test_item("1".to_string()));
        items.push(create_test_item("2".to_string()));
        items
    }

    #[test]
    fn test_guids_from_items() {
        let mut items = default_test_items();
        let mut expected: Vec<String> = Vec::new();
        expected.push("1".to_string());
        expected.push("2".to_string());

        // Items without guid should not be considered
        items.push(rss::Item::default());

        assert_eq!(guids_from_items(&items), expected);
    }

    #[derive(Clone)]
    struct MockFeed {
        items: Arc<RwLock<Vec<rss::Item>>>,
    }

    impl MockFeed {
        fn new() -> MockFeed {
            MockFeed {
                items: Arc::new(RwLock::new(default_test_items())),
            }
        }

        fn add_item(&self, item: rss::Item) {
            let mut w = self.items.write().unwrap();
            w.push(item);
        }
    }

    impl RSSFeed for MockFeed {
        fn get_config(&self) -> FeedConfig {
            FeedConfig {
                url: "".to_string(),
                webhooks: Some(vec!["".to_string()]),
            }
        }

        fn get_items(&self) -> Result<Vec<rss::Item>> {
            let r = self.items.read().unwrap();
            Ok(r.clone())
        }
    }

    #[test]
    fn test_poll() {
        let (tx, rx): (mpsc::Sender<FeedItem>, mpsc::Receiver<FeedItem>) = mpsc::channel();
        let feed = MockFeed::new();
        let feed_t = feed.clone();

        thread::spawn(move || poll(&feed_t, tx, time::Duration::from_secs(1)));

        // Give thread a moment to start
        thread::sleep(time::Duration::from_millis(200));

        // An item with guid "2" exists already, i.e. this one should not be send/received
        feed.add_item(create_test_item("2".to_string()));

        let item_new = create_test_item("3".to_string());

        feed.add_item(item_new.clone());

        let item_received = rx.recv().unwrap();

        assert_eq!(item_received.item, item_new);
    }
}
