#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(unused)]
#![warn(rust_2018_idioms)]

use std::io::stdin;
use std::sync::Mutex;
use std::thread;

use actix_web::{middleware, web, App, HttpRequest, HttpResponse, HttpServer};
use rss::{ChannelBuilder, Guid, ItemBuilder};
use uuid::Uuid;

async fn index(items: web::Data<Mutex<Vec<rss::Item>>>, _req: HttpRequest) -> HttpResponse {
    let items = items.lock().unwrap().clone();
    let channel = web::Data::new(
        ChannelBuilder::default()
            .title("feedserver")
            .link("http://localhost:4321")
            .items(items)
            .build()
            .unwrap(),
    );

    HttpResponse::Ok()
        .content_type("application/rss+xml")
        .body(channel.to_string())
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let items = web::Data::new(Mutex::new(Vec::new()));
    let items_t = items.clone();

    thread::spawn(move || loop {
        let mut s = String::new();
        println!("Press Return to add new feed item ...");
        stdin().read_line(&mut s).expect("couln't read string");
        if let Some('\n') = s.chars().next_back() {
            s.pop();
        }
        if let Some('\r') = s.chars().next_back() {
            s.pop();
        }

        let uuid = Uuid::new_v4();

        let mut guid = Guid::default();
        guid.set_value(uuid.to_string());

        let item = ItemBuilder::default().title(s).guid(guid).build().unwrap();

        items_t.lock().unwrap().insert(0, item);
    });

    HttpServer::new(move || {
        App::new()
            .app_data(items.clone())
            .wrap(middleware::Logger::default())
            .service(web::resource("/").to(index))
    })
    .bind("127.0.0.1:4321")?
    .run()
    .await
}
