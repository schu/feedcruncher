use crate::prelude::*;

use anyhow::Result;
use sqlx::{Pool, Sqlite};

#[derive(Debug)]
pub struct Notification {
    pub feed_item_id: i64,
    pub webhook_id: i64,
}

impl Notification {
    pub async fn save(&self, db: &Pool<Sqlite>, sent: bool) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO notifications (feed_item_id, webhook_id, sent)
            VALUES (?, ?, ?)
            ON CONFLICT (feed_item_id, webhook_id) DO NOTHING
            "#,
            self.feed_item_id,
            self.webhook_id,
            sent,
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn mark_as_sent(&self, db: &Pool<Sqlite>) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE notifications
            SET sent = true, sent_at = CURRENT_TIMESTAMP
            WHERE feed_item_id = ? AND webhook_id = ?
            "#,
            self.feed_item_id,
            self.webhook_id,
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn send(&self, db: &Pool<Sqlite>) -> Result<()> {
        let feed_item = FeedItems::get(db, self.feed_item_id).await?;
        let webhook = Webhooks::get(db, self.webhook_id).await?;

        webhook.push(feed_item).await?;

        self.mark_as_sent(db).await
    }
}

#[derive(Debug)]
pub struct Notifications {}

impl Notifications {
    pub async fn get_unsent(db: &Pool<Sqlite>) -> Result<Vec<Notification>> {
        let result = sqlx::query!(
            r#"
            SELECT feed_item_id, webhook_id
            FROM notifications
            WHERE sent = false
            "#,
        )
        .fetch_all(db)
        .await?;

        Ok(result
            .iter()
            .map(|row| Notification {
                feed_item_id: row.feed_item_id,
                webhook_id: row.webhook_id,
            })
            .collect())
    }
}
