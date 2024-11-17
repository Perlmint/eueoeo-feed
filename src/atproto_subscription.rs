use std::sync::Arc;

use crate::lexicon::com::atproto::sync::subscribe_repos::OutputSchema as RepoEvent;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use dagcbor::de::DeserializeOption;
use futures_util::StreamExt;
use log::{error, info};
use serde_ipld_dagcbor as dagcbor;
use sqlx::SqlitePool;
use tokio::{sync::watch, task::JoinHandle};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, thiserror::Error)]
#[error("subscription error")]
enum SubscriptionError {
    Fatal(anyhow::Error),
    Recoverable(anyhow::Error),
}

impl SubscriptionError {
    pub fn fatal<E: Into<anyhow::Error>>(e: E) -> Self {
        Self::Fatal(e.into())
    }

    pub fn recoverable<E: Into<anyhow::Error>>(e: E) -> Self {
        Self::Recoverable(e.into())
    }
}

#[async_trait]
pub trait FirehoseSubscriptionHandler {
    async fn handle_event(&self, event: RepoEvent) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct FirehoseSubscription<H: Sized + Clone> {
    handler: H,
    db: SqlitePool,
    service: Arc<String>,
    stop_rx: watch::Receiver<bool>,
    stop_tx: Arc<watch::Sender<bool>>,
}

impl<H: FirehoseSubscriptionHandler + Sized + Send + Sync + Clone + 'static>
    FirehoseSubscription<H>
{
    pub async fn new(
        db: SqlitePool,
        service: String,
        handler: H,
        stop_tx: Arc<watch::Sender<bool>>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            handler,
            db,
            service: Arc::new(service),
            stop_rx: stop_tx.subscribe(),
            stop_tx,
        })
    }

    pub fn run(&self) -> anyhow::Result<JoinHandle<anyhow::Result<()>>> {
        let mut subscription = self.clone();

        Ok(tokio::spawn(async move {
            if *subscription.stop_rx.borrow_and_update() {
                return Ok(());
            }

            loop {
                match subscription.loop_unit().await {
                    Ok(true) => {}
                    Ok(false) => {
                        break;
                    }
                    Err(e) => {
                        error!("Subscription connect is broken. retry later: {e:?}");
                        if let SubscriptionError::Fatal(_) = &e {
                            let _ = subscription.stop_tx.send(true);
                            return Err(e).context("Stop subscription by fatal error");
                        } else {
                            const DELAY: std::time::Duration = std::time::Duration::from_secs(5);
                            log::info!("Restart loop after {DELAY:?}");
                            tokio::time::sleep(DELAY).await;
                        }
                    }
                };
            }

            Ok(())
        }))
    }

    async fn loop_unit(&mut self) -> Result<bool, SubscriptionError> {
        // TODO: reconnection
        let mut url = url::Url::parse(&self.service)
            .context("Failed to parse url")
            .map_err(SubscriptionError::fatal)?;
        unsafe { url.path_segments_mut().unwrap_unchecked() }
            .push("xrpc")
            .push(crate::lexicon::com::atproto::sync::subscribe_repos::ID);
        if let Some(cursor) = self
            .get_cursor()
            .await
            .context("Failed to get previous received position from DB")
            .map_err(SubscriptionError::fatal)?
        {
            url.query_pairs_mut()
                .append_pair("cursor", &cursor.to_string());
        }
        let (stream, _) = tokio_tungstenite::connect_async(url.to_string())
            .await
            .with_context(|| format!("Failed to connect to service({url})"))
            .map_err(SubscriptionError::fatal)?;
        let (_tx, mut rx) = stream.split();

        while let Some(ret) = rx.next().await {
            if self
                .stop_rx
                .has_changed()
                .map_err(SubscriptionError::fatal)?
                && *self.stop_rx.borrow_and_update()
            {
                info!("Stop processing subscription by stop signal");
                return Ok(false);
            }
            let message = ret
                .context("Failed to receive message")
                .map_err(SubscriptionError::fatal)?;
            if let Message::Binary(data) = message {
                let event = Self::parse_message(&data)
                    .context("Failed to parse message")
                    .map_err(SubscriptionError::fatal)?;

                let cursor = if let RepoEvent::Commit(commit) = &event {
                    let seq = commit._common.seq;
                    (seq % 20 == 0).then_some(seq)
                } else {
                    None
                };

                self.handler
                    .handle_event(event)
                    .await
                    .map_err(SubscriptionError::fatal)?;

                if let Some(cursor) = cursor {
                    self.update_cursor(cursor)
                        .await
                        .map_err(SubscriptionError::recoverable)?;
                }
            }
        }

        Ok(true)
    }

    pub fn parse_message(data: &Vec<u8>) -> anyhow::Result<RepoEvent> {
        let mut cursor = std::io::Cursor::new(&data);

        let header: Header = dagcbor::from_reader_with_option(
            &mut cursor,
            DeserializeOption {
                ignore_trailing: true,
            },
        )
        .context("Failed to parse header")?;
        let body = &data[(cursor.position() as usize)..];
        if body.is_empty() {
            return Err(anyhow!("message has no body"));
        }
        match header.operation {
            HeaderOperation::Ok => {
                Ok(RepoEvent::from_cbor(&header._type, body).context("Failed to parse event")?)
            }
            HeaderOperation::Error => Err(anyhow::anyhow!(
                "Error received from subscription - {}",
                dagcbor::from_slice::<Error>(body)
                    .context("Failed to parse error")?
                    .error_type
            )),
        }
    }

    async fn update_cursor(&self, cursor: u64) -> anyhow::Result<()> {
        let cursor = cursor as i64;
        sqlx::query!(
            r#"
            INSERT INTO `app_state` (
                `key`, `value`
            ) VALUES (
                "bsky_cursor", ?
            ) ON CONFLICT (`key`) DO UPDATE SET
                `value`=`excluded`.`value`
            WHERE
                `key` = "bsky_cursor"
        "#,
            cursor
        )
        .execute(&self.db)
        .await?;
        info!("Cursor updated: {cursor}");

        Ok(())
    }

    async fn get_cursor(&self) -> anyhow::Result<Option<u64>> {
        sqlx::query!(
            r#"
            SELECT `value` FROM `app_state` WHERE `key` = "bsky_cursor"
        "#
        )
        .fetch_optional(&self.db)
        .await
        .context("Failed to get cursor")
        .map(|v| v.map(|v| serde_json::from_str::<u64>(&v.value).unwrap()))
    }
}

#[derive(Debug, serde::Deserialize)]
struct Header {
    #[serde(rename = "op")]
    pub operation: HeaderOperation,
    #[serde(rename = "t")]
    pub _type: String,
}

#[derive(Debug, serde_repr::Deserialize_repr, PartialEq)]
#[repr(i8)]
enum HeaderOperation {
    Ok = 1,
    Error = -1,
}

#[derive(Debug, serde::Deserialize)]
pub struct Error {
    #[serde(rename = "error")]
    pub error_type: String,
    pub message: Option<String>,
}

pub enum SubscriptionMessage<T> {
    Message(T),
    Error(Error),
}
