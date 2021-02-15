use crate::schema::*;
use diesel::prelude::*;

#[derive(Queryable)]
pub struct Feed {
    pub id: i32,
    pub url: String,
    pub last_fetched_at: String,
}

#[derive(Insertable)]
#[table_name = "feeds"]
pub struct NewFeed<'a> {
    pub url: &'a str,
    pub last_fetched_at: &'a str,
}

#[derive(Queryable, Debug)]
pub struct Item {
    pub id: i32,
    pub guid: String,
    pub link: String,
    pub title: Option<String>,
    pub fetched_at: String,
    pub feed: i32,
}

#[derive(Insertable)]
#[table_name = "items"]
pub struct NewItem<'a> {
    pub guid: &'a str,
    pub link: &'a str,
    pub title: &'a str,
    pub fetched_at: &'a str,
    pub feed: &'a i32,
}

#[derive(Queryable, Debug)]
pub struct Webhook {
    pub id: i32,
    pub url: String,
}

#[derive(Insertable)]
#[table_name = "webhooks"]
pub struct NewWebhook<'a> {
    pub url: &'a str,
}

#[derive(Queryable, Debug, PartialEq)]
pub struct Notification {
    pub id: i32,
    pub item: i32,
    pub webhook: i32,
    pub sent: i32,
    pub sent_at: Option<String>,
}

#[derive(Insertable)]
#[table_name = "notifications"]
pub struct NewNotification<'a> {
    pub item: &'a i32,
    pub webhook: &'a i32,
    pub sent: &'a i32,
    pub sent_at: &'a str,
}
