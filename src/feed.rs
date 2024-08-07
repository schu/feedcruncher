use core::fmt::Debug;
use std::fmt;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use tokio::sync::Mutex;

#[async_trait]
pub trait Feed: Send + Sync + 'static {
    fn kind(&self) -> String;
    fn url(&self) -> String;
    fn db(&self) -> Result<Pool<Sqlite>>;

    async fn fetch(&self) -> Result<Vec<FeedItem>>;

    async fn is_new(&self) -> Result<bool> {
        let feed_id = self.id().await?;

        let result = sqlx::query!(
            r#"
            SELECT is_new
            FROM feeds
            WHERE id = ?
            "#,
            feed_id,
        )
        .fetch_one(&self.db()?)
        .await?;

        Ok(result.is_new)
    }

    async fn set_is_new(&self, is_new: bool) -> Result<()> {
        let feed_id = self.id().await?;

        sqlx::query!(
            r#"
            UPDATE feeds
            SET is_new = ?
            WHERE id = ?
            "#,
            is_new,
            feed_id,
        )
        .execute(&self.db()?)
        .await?;

        Ok(())
    }

    async fn id(&self) -> Result<i64> {
        let u = self.url();

        let result = sqlx::query!(
            r#"
            SELECT id
            FROM feeds
            WHERE url = ?
            "#,
            u,
        )
        .fetch_one(&self.db()?)
        .await?;

        Ok(result.id)
    }

    async fn save(&self) -> Result<()> {
        let k = self.kind();
        let u = self.url();

        sqlx::query!(
            r#"
            INSERT INTO feeds (kind, url, is_new)
            VALUES (?, ?, true)
            ON CONFLICT (url) DO NOTHING
            "#,
            k,
            u,
        )
        .execute(&self.db()?)
        .await?;

        Ok(())
    }

    fn webhook_urls(&self) -> Option<Vec<String>>;
}

impl Debug for dyn Feed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "feed – kind: '{}' url: {}", self.kind(), self.url())
    }
}

#[derive(Debug, Clone)]
pub struct RSSFeed {
    db: Pool<Sqlite>,
    url: String,
    webhook_urls: Option<Vec<String>>,
}

impl RSSFeed {
    pub fn new(url: String, webhook_urls: Option<Vec<String>>, db: &Pool<Sqlite>) -> Result<Self> {
        Ok(Self {
            db: db.clone(),
            url,
            webhook_urls,
        })
    }
}

#[async_trait]
impl Feed for RSSFeed {
    fn kind(&self) -> String {
        "rss".to_string()
    }

    fn url(&self) -> String {
        self.url.clone()
    }

    fn db(&self) -> Result<Pool<Sqlite>> {
        Ok(self.db.clone())
    }

    fn webhook_urls(&self) -> Option<Vec<String>> {
        self.webhook_urls.clone()
    }

    async fn fetch(&self) -> Result<Vec<FeedItem>> {
        let response = reqwest::get(&self.url).await?.text().await?;
        let rssfeed = rss::Channel::read_from(response.as_bytes())?;

        let feed: Arc<Mutex<Box<dyn Feed>>> =
            Arc::new(Mutex::new(Box::new(self.clone()) as Box<dyn Feed>));

        let items = rssfeed
            .into_items()
            .iter()
            .map(|item| {
                FeedItem {
                    db: self.db.clone(),
                    feed: feed.clone(),
                    // TODO: what if we receive malformed data w/o guid or link?
                    // Is there a better way (w/o unwrap) to handle this?
                    guid: item.guid().unwrap().value().to_string(),
                    link: item.link().unwrap().to_string(),
                    title: item.title().map(|s| s.to_string()),
                    published_at: item.pub_date().map(|d| d.to_string()),
                }
            })
            .collect();

        Ok(items)
    }
}

#[derive(Debug, Clone)]
pub struct AtomFeed {
    db: Pool<Sqlite>,
    url: String,
    webhook_urls: Option<Vec<String>>,
}

impl AtomFeed {
    pub fn new(url: String, webhook_urls: Option<Vec<String>>, db: &Pool<Sqlite>) -> Result<Self> {
        Ok(Self {
            db: db.clone(),
            url,
            webhook_urls,
        })
    }
}

#[async_trait]
impl Feed for AtomFeed {
    fn kind(&self) -> String {
        "atom".to_string()
    }

    fn url(&self) -> String {
        self.url.clone()
    }

    fn db(&self) -> Result<Pool<Sqlite>> {
        Ok(self.db.clone())
    }

    fn webhook_urls(&self) -> Option<Vec<String>> {
        self.webhook_urls.clone()
    }

    async fn fetch(&self) -> Result<Vec<FeedItem>> {
        let response = reqwest::get(&self.url).await?.text().await?;
        let atomfeed = atom_syndication::Feed::read_from(response.as_bytes())?;

        let feed: Arc<Mutex<Box<dyn Feed>>> =
            Arc::new(Mutex::new(Box::new(self.clone()) as Box<dyn Feed>));

        let items: Vec<FeedItem> = atomfeed
            .entries()
            .iter()
            .map(|entry| FeedItem {
                db: self.db.clone(),
                feed: feed.clone(),
                guid: entry.id().to_string(),
                link: entry.links().first().unwrap().href().to_string(),
                title: Some(entry.title.value.clone()),
                published_at: entry.published().map(|d| d.to_string()),
            })
            .collect();

        Ok(items)
    }
}

pub struct FeedItem {
    pub db: Pool<Sqlite>,
    pub feed: Arc<Mutex<Box<dyn Feed>>>,
    pub guid: String,
    pub link: String,
    pub title: Option<String>,
    pub published_at: Option<String>,
}

impl Debug for FeedItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "feed item – guid: '{}' link: {}", self.guid, self.link)
    }
}

impl FeedItem {
    pub async fn save(&self) -> Result<()> {
        let feed_id = self.feed.lock().await.id().await?;

        sqlx::query!(
            r#"
            INSERT INTO feed_items (feed_id, guid, link, title, published_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (guid, link) DO NOTHING
            "#,
            feed_id,
            self.guid,
            self.link,
            self.title,
            self.published_at,
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn id(&self) -> Result<i64> {
        let result = sqlx::query!(
            r#"
            SELECT id
            FROM feed_items
            WHERE guid = ?
            "#,
            self.guid,
        )
        .fetch_one(&self.db)
        .await?;

        Ok(result.id)
    }
}

pub struct FeedItems {}

impl FeedItems {
    pub async fn get(db: &Pool<Sqlite>, id: i64) -> Result<FeedItem> {
        let result = sqlx::query!(
            r#"
            SELECT guid, link, title, published_at
            FROM feed_items
            WHERE id = ?
            "#,
            id,
        )
        .fetch_one(db)
        .await?;

        Ok(FeedItem {
            db: db.clone(),
            feed: Arc::new(Mutex::new(
                Box::new(RSSFeed::new("".to_string(), None, db)?) as Box<dyn Feed>,
            )),
            guid: result.guid,
            link: result.link,
            title: result.title,
            published_at: result.published_at.map(|dt| dt.to_string()),
        })
    }
}
