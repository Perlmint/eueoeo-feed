use std::collections::HashMap;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use crossbeam::channel::Sender;
use futures_util::io::Cursor;
use sqlx::SqlitePool;

use crate::atproto_subscription::FirehoseSubscriptionHandler;

use crate::lexicon::com::atproto::sync::subscribe_repos::RepoOpAction;
use crate::lexicon::AtUri;
use crate::lexicon::{
    app::bsky::feed::{
        like::Record as LikeRecord, post::Record as PostRecord, repost::Record as RepostRecord,
    },
    com::atproto::sync::subscribe_repos::OutputSchema as RepoEvent,
};

#[serde_with::serde_as]
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UserProfile {
    pub name: String,
    pub avatar: String,
    #[serde_as(as = "serde_with::TimestampSeconds<i64>")]
    pub last_cached: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct ServiceSubscriptionHandler {
    db: SqlitePool,
    sse_sender: Sender<UserProfile>,
    user_profiles: scc::hash_map::HashMap<String, UserProfile>,
}

impl ServiceSubscriptionHandler {
    pub fn new(db: SqlitePool, sse_sender: Sender<UserProfile>) -> Self {
        Self {
            db,
            sse_sender,
            user_profiles: scc::hash_map::HashMap::new(),
        }
    }
}

#[async_trait]
impl FirehoseSubscriptionHandler for ServiceSubscriptionHandler {
    async fn handle_event(&self, event: RepoEvent) -> anyhow::Result<()> {
        let RepoEvent::Commit(event) = event else {
            return Ok(());
        };

        let mut blocks = Cursor::new(event.blocks);
        let (blocks, _header) = rs_car::car_read_all(&mut blocks, false)
            .await
            .context("Failed to parse blocks")?;
        let blocks: HashMap<_, _> = blocks.into_iter().collect();
        let author = event.repo;

        for op in event.ops {
            match op.action {
                RepoOpAction::Create => {
                    let Some(cid) = &op.cid else {
                        continue;
                    };
                    let block = blocks
                        .get(cid)
                        .ok_or_else(|| anyhow!("Cannot find block of cid on op"))?;
                    let item: Record =
                        serde_ipld_dagcbor::from_slice(&block).context("Failed to parse block")?;
                    if let Record::Post(post) = item {
                        if post.text == "으어어" {
                            let uri = AtUri::with_auth_path(author.clone(), op.path).to_string();
                            let cid = cid.to_string();
                            let now = chrono::Utc::now().naive_utc();
                            sqlx::query!(
                                r#"
                                INSERT INTO `post` (
                                    `uri`, `cid`, `author`, `indexedAt`
                                ) VALUES (
                                    ?, ?, ?, ?
                                ) ON CONFLICT DO NOTHING
                            "#,
                                uri,
                                cid,
                                author,
                                now
                            )
                            .execute(&self.db)
                            .await?;

                            // self.sse_sender.send(event.repo.clone())?;
                        }
                    }
                }
                RepoOpAction::Update => { /* Not supported yet */ }
                RepoOpAction::Delete => {
                    let uri = AtUri::with_auth_path(author.clone(), op.path).to_string();
                    sqlx::query!("DELETE FROM `post` where uri = ?", uri)
                        .execute(&self.db)
                        .await?;
                }
            }
        }

        Ok(())
    }
}
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "$type")]
pub enum Record {
    #[serde(rename = "app.bsky.feed.post")]
    Post(PostRecord),
    #[serde(rename = "app.bsky.feed.repost")]
    RePost(RepostRecord),
    #[serde(rename = "app.bsky.feed.like")]
    Like(LikeRecord),
    #[serde(other)]
    Unknown,
}

struct Ops<T> {
    pub creates: Vec<T>,
    pub deletes: Vec<T>,
}

struct OpsByType {
    posts: Ops<PostRecord>,
}
