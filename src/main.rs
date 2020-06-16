#![deny(unused, nonstandard_style, future_incompatible)]
#![warn(rust_2018_idioms)]

use std::sync::mpsc;
use std::thread;
use std::time;

use anyhow::{anyhow, Result};
use clap::Clap;
use reqwest;
use rss;
use serde::Serialize;
use serde_json;

#[derive(Clap, Debug)]
#[clap(version = "0.1.0")]
struct Opts {
    feed_urls: Vec<String>,
    #[clap(short, long, default_value = "600")]
    sleep_dur: u64,
    #[clap(short, long)]
    webhook_url: String,
}

fn main() {
    let opts: Opts = Opts::parse();
    let sleep_dur = opts.sleep_dur;
    let (tx, rx): (mpsc::Sender<rss::Item>, mpsc::Receiver<rss::Item>) = mpsc::channel();

    println!("Watching {:#?}", opts.feed_urls);

    for feed_url in opts.feed_urls {
        let feed = FeedReqwest::new(feed_url);
        let tx = tx.clone();
        thread::spawn(move || poll(&feed, tx, time::Duration::from_secs(sleep_dur)));
    }

    let webhook = WebhookDiscord::new(opts.webhook_url);

    loop {
        println!("Waiting for new feed items ...");
        let received = match rx.recv() {
            Ok(received) => received,
            Err(e) => {
                println!("failed to receive message: {}", e);
                continue;
            }
        };
        println!("{:#?}", received);
        match webhook.push(&received) {
            Ok(_) => (),
            Err(e) => {
                println!("failed to push message: {}", e);
            }
        };
    }
}

trait RSSFeed {
    fn get_items(&self) -> Result<Vec<rss::Item>>;
}

struct FeedReqwest {
    url: String,
}

impl FeedReqwest {
    fn new(url: String) -> FeedReqwest {
        FeedReqwest { url }
    }
}

impl RSSFeed for FeedReqwest {
    fn get_items(&self) -> Result<Vec<rss::Item>> {
        let res = reqwest::blocking::get(&self.url)?;
        let feed_xml = res.text()?;
        Ok(rss::Channel::read_from(feed_xml.as_bytes())?.into_items())
    }
}

trait Webhook {
    fn push(&self, item: &rss::Item) -> Result<()>;
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

    fn render_message(&self, item: &rss::Item) -> Result<String> {
        let msg_title = match item.title() {
            Some(t) => format!("{}\n", t.to_string()),
            None => "".to_string(),
        };
        let msg_content = match item.guid() {
            Some(g) => format!("{}{}", msg_title, g.value().to_string()),
            None => return Err(anyhow!("got item without guid")),
        };
        let msg = DiscordMessage {
            content: msg_content,
        };
        Ok(serde_json::to_string(&msg)?)
    }
}

impl Webhook for WebhookDiscord {
    fn push(&self, item: &rss::Item) -> Result<()> {
        let msg = self.render_message(item)?;
        reqwest::blocking::Client::new()
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(msg)
            .send()?;
        Ok(())
    }
}

fn guids_from_items(items: &Vec<rss::Item>) -> Vec<String> {
    items
        .iter()
        .filter(|item| match item.guid() {
            Some(_) => true,
            None => false,
        })
        .map(|item| match item.guid() {
            Some(guid) => guid.value().to_string(),
            None => panic!("cannot happen"),
        })
        .collect()
}

fn poll(feed: &impl RSSFeed, tx: mpsc::Sender<rss::Item>, sleep_dur: time::Duration) {
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
                    if feed_guids.contains(&guid_val) || guid_val.is_empty() {
                        false
                    } else {
                        true
                    }
                }
                None => false,
            })
            .collect();

        // Send new items to receiver thread
        for item in new_items {
            // Items without guid were filtered out above, i.e. safe to unwrap
            let guid = item.guid().unwrap().value().to_string();

            match tx.send(item) {
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
        fn get_items(&self) -> Result<Vec<rss::Item>> {
            let r = self.items.read().unwrap();
            Ok(r.clone())
        }
    }

    #[test]
    fn test_poll() {
        let (tx, rx): (mpsc::Sender<rss::Item>, mpsc::Receiver<rss::Item>) = mpsc::channel();
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

        assert_eq!(item_received, item_new);
    }
}
