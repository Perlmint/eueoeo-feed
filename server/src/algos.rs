use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::{config::Config, lexicon::app::bsky::feed::get_feed_skeleton};
mod eueoeo;

#[derive(Clone)]
pub struct Context {
    pub db: SqlitePool,
    pub config: Arc<Config>,
}

#[async_trait]
pub trait AlgoHandler {
    fn short_name(&self) -> &str;
    async fn handle(
        &self,
        context: Context,
        params: get_feed_skeleton::QueryParams,
    ) -> anyhow::Result<get_feed_skeleton::OutputSchema>;
}

pub type AlgoHandlers = HashMap<String, Box<dyn AlgoHandler + Send + Sync>>;

pub fn create() -> AlgoHandlers {
    type B = Box<dyn AlgoHandler + Send + Sync>;
    [Box::new(eueoeo::Handler) as B]
        .into_iter()
        .map(|h| (h.short_name().to_string(), h))
        .collect()
}
