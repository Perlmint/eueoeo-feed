use async_trait::async_trait;
use crossbeam::channel::Sender;
use log::{debug, warn};
use sqlx::SqlitePool;

use crate::{
    atproto_subscription::FirehoseSubscriptionHandler,
    lexicon::{
        com::atproto::sync::subscribe_repos::{OutputSchema as RepoEvent, Record, RepoOpAction},
        AtUri,
    },
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

        let Some(blocks) = event.blocks else {
            debug!("drop no-blocks commit event");
            return Ok(());
        };

        let blocks = blocks.parse().await?;

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
                    let item = block?;
                    if let Record::Post(post) = item {
                        debug!(r#"new post [{}] - """{}""""#, author, post.text);
                        if post.text == "으어어" {
                            let uri = AtUri::with_auth_path(author.clone(), op.path).to_string();
                            let cid = cid.to_string();
                            let now = chrono::Utc::now().to_rfc3339();
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
