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
    #[clap(short, long)]
    webhook_url: String,
}

type Result<T> = std::result::Result<T, String>;

fn main() {
    let opts: Opts = Opts::parse();
    let (tx, rx): (mpsc::Sender<String>, mpsc::Receiver<String>) = mpsc::channel();

    println!("Watching {:#?}", opts.feed_urls);

    for feed_url in opts.feed_urls {
        let tx = tx.clone();
        thread::spawn(move || poll(&feed_url, tx));
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
        println!("{}", received);
        match push_message(&opts.webhook_url, &received) {
            Ok(_) => (),
            Err(e) => {
                println!("failed to push message: {}", e);
            }
        };
    }
}

fn push_message(target_url: &String, msg: &String) -> Result<String> {
    match reqwest::blocking::Client::new()
        .post(target_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(format!("{{\"content\": \"{}\"}}", msg))
        .send()
    {
        Ok(res) => Ok(format!("{:#?}", res)),
        Err(e) => Err(e.to_string()),
    }
}

fn get_feed_items(feed_url: &String) -> Result<Vec<rss::Item>> {
    let res = match reqwest::blocking::get(feed_url) {
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

fn poll(feed_url: &String, tx: mpsc::Sender<String>) {
    let feed_items = get_feed_items(feed_url).unwrap();
    let mut feed_guids = guids_from_items(&feed_items);

    loop {
        thread::sleep(time::Duration::from_secs(600));

        let feed_items = match get_feed_items(feed_url) {
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
                    match tx.send(s.clone()) {
                        Ok(_) => (),
                        Err(e) => {
                            println!("failed to send message to receiver thread: {}", e);
                        }
                    };
                    feed_guids.push(s);
                }
                None => {
                    println!("got item without guid - skipping");
                    continue;
                }
            }
        }
    }
}
