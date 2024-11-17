use std::collections::HashMap;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use crossbeam::channel::Sender;
use futures_util::io::Cursor;
use log::{debug, warn};
use sqlx::SqlitePool;

use crate::atproto_subscription::FirehoseSubscriptionHandler;

use crate::lexicon::com::atproto::sync::subscribe_repos::RepoOpAction;
use crate::lexicon::AtUri;
use crate::lexicon::{
    app::bsky::{
        feed::{
            like::Record as LikeRecord, post::Record as PostRecord, repost::Record as RepostRecord,
        },
        graph::follow::Record as FollowRecord,
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

        let Some(blocks_bytes) = event.blocks else {
            debug!("drop no-blocks commit event");
            return Ok(());
        };

        let mut blocks = Cursor::new(&blocks_bytes);
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
                    let Some(block) = blocks.get(cid) else {
                        warn!(
                            "Could not find block of cid({cid}) on op. block_keys: {}",
                            itertools::join(blocks.keys(), ", ")
                        );
                        continue;
                    };
                    let item: Record =
                        serde_ipld_dagcbor::from_slice(&block).with_context(|| {
                            let human_readable = if let Ok(v) =
                                serde_ipld_dagcbor::from_slice::<serde_json::Value>(&block)
                            {
                                v.to_string()
                            } else {
                                format!("{block:?}")
                            };
                            format!("Failed to parse block - {human_readable}")
                        })?;
                    if let Record::Post(post) = item {
                        debug!(r#"new post [{}] - """{}""""#, author, post.text);
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
    #[serde(rename = "app.bsky.graph.follow")]
    Follow(FollowRecord),
    #[serde(other)]
    Unknown,
}

struct Ops<T> {
    pub creates: Vec<T>,
    pub deletes: Vec<T>,
}

struct OpsByType {
    posts: Ops<PostRecord>,
    reposts: Ops<RepostRecord>,
    likes: Ops<LikeRecord>,
    follows: Ops<FollowRecord>,
}
