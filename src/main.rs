use std::sync::mpsc;
use std::thread;
use std::time;

extern crate reqwest;

extern crate rss;

extern crate clap;
use clap::Clap;

#[derive(Clap, Debug)]
#[clap(version = "0.1.0")]
struct Opts {
    feed_urls: Vec<String>,
    #[clap(short, long, default_value = "600")]
    sleep_dur: u64,
    #[clap(short, long)]
    webhook_url: String,
}

type Result<T> = std::result::Result<T, String>;

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
        match push_message(&opts.webhook_url, &received) {
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
        let res = match reqwest::blocking::get(&self.url) {
            Ok(r) => r,
            Err(e) => return Err(e.to_string()),
        };

        let feed_xml = match res.text() {
            Ok(s) => s,
            Err(e) => return Err(e.to_string()),
        };

        match rss::Channel::read_from(feed_xml.as_bytes()) {
            Ok(channel) => Ok(channel.into_items()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn push_message(target_url: &String, item: &rss::Item) -> Result<String> {
    let guid = match item.guid() {
        Some(guid) => guid.value().to_string(),
        None => return Err("got item without guid".to_string()),
    };

    match reqwest::blocking::Client::new()
        .post(target_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(format!("{{\"content\": \"{}\"}}", guid))
        .send()
    {
        Ok(res) => Ok(format!("{:#?}", res)),
        Err(e) => Err(e.to_string()),
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

        let feed_items = match feed.get_items() {
            Ok(items) => items,
            Err(e) => {
                println!("failed to get feed: {}", e);
                continue;
            }
        };

        for item in feed_items {
            match item.guid() {
                Some(guid) => {
                    let s = guid.value().to_string();
                    if feed_guids.contains(&s) {
                        continue;
                    }
                    match tx.send(item) {
                        Ok(_) => {
                            feed_guids.push(s);
                        }
                        Err(e) => {
                            println!("failed to send message to receiver thread: {}", e);
                        }
                    };
                }
                None => {
                    println!("got item without guid - skipping");
                    continue;
                }
            }
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

        let item_new = create_test_item("3".to_string());

        feed.add_item(item_new.clone());

        let item_received = rx.recv().unwrap();

        assert_eq!(item_received, item_new);
    }
}
