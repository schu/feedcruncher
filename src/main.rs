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

fn main() {
    let opts: Opts = Opts::parse();
    let (tx, rx): (mpsc::Sender<String>, mpsc::Receiver<String>) = mpsc::channel();

    for feed_url in opts.feed_urls {
        let tx = tx.clone();
        thread::spawn(move || poll(&feed_url, tx));
    }

    loop {
        println!("Waiting for new feed items ...");
        let received = rx.recv().unwrap();
        println!("{}", received);
        push_message(&opts.webhook_url, &received);
    }
}

fn push_message(target_url: &String, msg: &String) {
    let client = reqwest::blocking::Client::new();
    let res = client
        .post(target_url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(format!("{{\"content\": \"{}\"}}", msg))
        .send()
        .unwrap();

    println!("{:#?}", res);
}

fn get_feed_items(feed_url: &String) -> Vec<rss::Item> {
    let feed_xml = reqwest::blocking::get(feed_url).unwrap().text().unwrap();

    rss::Channel::read_from(feed_xml.as_bytes())
        .unwrap()
        .into_items()
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
    let feed_items = get_feed_items(feed_url);
    let mut feed_guids = guids_from_items(&feed_items);

    loop {
        let feed_items = get_feed_items(feed_url);
        for item in feed_items {
            match item.guid() {
                Some(guid) => {
                    let s = guid.value().to_string();
                    if feed_guids.contains(&s) {
                        continue;
                    }
                    tx.send(s.clone()).unwrap();
                    feed_guids.push(s);
                }
                None => {
                    println!("got item without guid - skipping");
                    continue;
                }
            }
        }

        thread::sleep(time::Duration::from_secs(600));
    }
}
