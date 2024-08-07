use crate::prelude::*;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::Serialize;
use sqlx::{Pool, Sqlite};

#[async_trait]
pub trait Webhook: Send + Sync + 'static {
    async fn push(&self, item: FeedItem) -> Result<()>;
    async fn save(&self, db: &Pool<Sqlite>) -> Result<i64>;
    fn url(&self) -> String;
}

pub struct WebhookDiscord {
    pub url: String,
}

#[derive(Serialize)]
struct DiscordMessage {
    content: String,
}

impl WebhookDiscord {
    fn render_message(&self, item: &FeedItem) -> Result<String> {
        let msg_title = match &item.title {
            Some(t) => format!("{}\n\n", t),
            None => "".to_string(),
        };
        let msg_content = format!("{}{}", msg_title, item.link);
        let msg = DiscordMessage {
            content: msg_content,
        };
        Ok(serde_json::to_string(&msg)?)
    }
}

#[async_trait]
impl Webhook for WebhookDiscord {
    async fn push(&self, item: FeedItem) -> Result<()> {
        let message = self.render_message(&item)?;

        let client = reqwest::Client::new();
        client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(message)
            .send()
            .await?;

        Ok(())
    }

    async fn save(&self, db: &Pool<Sqlite>) -> Result<i64> {
        sqlx::query!(
            r#"
            INSERT INTO webhooks (url)
            VALUES (?)
            ON CONFLICT (url) DO NOTHING
            "#,
            self.url,
        )
        .execute(db)
        .await?;

        let result = sqlx::query!(
            r#"
            SELECT id FROM webhooks WHERE url = ?
            "#,
            self.url,
        )
        .fetch_one(db)
        .await?;

        Ok(result.id)
    }

    fn url(&self) -> String {
        self.url.clone()
    }
}

// TODO: currently untested, as I'm not using Slack;
// if you do, please test this and let me know if it
// works as expected!
pub struct WebhookSlack {
    pub url: String,
}

#[derive(Serialize)]
struct SlackMessage {
    text: String,
}

impl WebhookSlack {
    fn render_message(&self, item: &FeedItem) -> Result<String> {
        let msg_title = match &item.title {
            Some(t) => format!("{}\n\n", t),
            None => "".to_string(),
        };
        let msg_content = format!("{}{}", msg_title, item.link);
        let msg = SlackMessage { text: msg_content };
        Ok(serde_json::to_string(&msg)?)
    }
}

#[async_trait]
impl Webhook for WebhookSlack {
    async fn push(&self, item: FeedItem) -> Result<()> {
        let message = self.render_message(&item)?;

        let client = reqwest::Client::new();
        client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(message)
            .send()
            .await?;

        Ok(())
    }

    async fn save(&self, db: &Pool<Sqlite>) -> Result<i64> {
        sqlx::query!(
            r#"
            INSERT INTO webhooks (url)
            VALUES (?)
            ON CONFLICT (url) DO NOTHING
            "#,
            self.url,
        )
        .execute(db)
        .await?;

        let result = sqlx::query!(
            r#"
            SELECT id FROM webhooks WHERE url = ?
            "#,
            self.url,
        )
        .fetch_one(db)
        .await?;

        Ok(result.id)
    }

    fn url(&self) -> String {
        self.url.clone()
    }
}

pub struct WebhookNoop {
    pub url: String,
}

#[async_trait]
impl Webhook for WebhookNoop {
    async fn push(&self, item: FeedItem) -> Result<()> {
        println!("{:#?}", item);

        Ok(())
    }

    async fn save(&self, db: &Pool<Sqlite>) -> Result<i64> {
        sqlx::query!(
            r#"
            INSERT INTO webhooks (url)
            VALUES (?)
            ON CONFLICT (url) DO NOTHING
            "#,
            self.url,
        )
        .execute(db)
        .await?;

        let result = sqlx::query!(
            r#"
            SELECT id FROM webhooks WHERE url = ?
            "#,
            self.url,
        )
        .fetch_one(db)
        .await?;

        Ok(result.id)
    }

    fn url(&self) -> String {
        self.url.clone()
    }
}

pub struct Webhooks {}

impl Webhooks {
    pub async fn get(db: &Pool<Sqlite>, id: i64) -> Result<Box<dyn Webhook>> {
        let result = sqlx::query!(
            r#"
            SELECT url
            FROM webhooks
            WHERE id = ?
            "#,
            id,
        )
        .fetch_one(db)
        .await?;

        webhook_from_url(result.url)
    }
}

pub fn webhook_from_url(url: String) -> Result<Box<dyn Webhook>> {
    if url == "-" {
        return Ok(Box::new(WebhookNoop { url }));
    }
    if url.contains("https://discordapp.com/api") || url.contains("https://discord.com/api") {
        return Ok(Box::new(WebhookDiscord { url }));
    }
    if url.contains("https://hooks.slack.com") {
        return Ok(Box::new(WebhookSlack { url }));
    }
    Err(anyhow!("unknown webhook target: '{}'", url))
}
