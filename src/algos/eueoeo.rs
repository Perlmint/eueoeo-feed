use anyhow::Context as _;
use async_trait::async_trait;
use chrono::Utc;

use crate::{data::Post, lexicon::app::bsky::feed::get_feed_skeleton};

use super::{AlgoHandler, Context};

pub struct Handler;

#[async_trait]
impl AlgoHandler for Handler {
    fn short_name(&self) -> &str {
        "eueoeo"
    }

    async fn handle(
        &self,
        context: Context,
        params: get_feed_skeleton::QueryParams,
    ) -> anyhow::Result<get_feed_skeleton::OutputSchema> {
        let feed = if let Some(cursor) = params.cursor {
            let (indexed_at, cid) = cursor.split_once("::").context("malformed cursor")?;
            let time = indexed_at
                .parse::<i64>()
                .context("malformed cursor - invalid indexedAt part")?;
            let time = chrono::DateTime::<Utc>::from_timestamp_millis(time)
                .context("malformed cursor - invalid indexedAt part")?
                .to_rfc3339();

            sqlx::query_as!(
                Post,
                r#"
                SELECT * FROM `post`
                    WHERE `indexedAt` < ? OR (
                        `indexedAt` = ? AND `cid` < ?
                    )
                    ORDER BY `indexedAt` DESC, `cid` DESC
                    LIMIT ?
                "#,
                time,
                time,
                cid,
                params.limit
            )
            .fetch_all(&context.db)
            .await?
        } else {
            sqlx::query_as!(
                Post,
                r#"
                SELECT * FROM `post`
                    ORDER BY `indexedAt` DESC, `cid` DESC
                    LIMIT ?
                "#,
                params.limit
            )
            .fetch_all(&context.db)
            .await?
        };

        let cursor = feed.last().map(|last| {
            let timestamp = chrono::DateTime::parse_from_rfc3339(unsafe {
                last.indexedAt.as_ref().unwrap_unchecked()
            })
            .unwrap()
            .timestamp_millis();
            format!("{}::{}", timestamp, unsafe {
                last.cid.as_ref().unwrap_unchecked()
            })
        });

        let feed = feed
            .into_iter()
            .map(|f| get_feed_skeleton::Feed {
                post: unsafe { f.uri.unwrap_unchecked() },
            })
            .collect();

        Ok(get_feed_skeleton::OutputSchema { cursor, feed })
    }
}
