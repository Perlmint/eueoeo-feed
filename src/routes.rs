use axum::Router;

use crate::{algos::AlgoHandlers, config::Config};

mod stream;
mod well_known;
mod xrpc;

pub fn create_router<S: Clone + Send + Sync + 'static>(
    config: &Config,
    algos: AlgoHandlers,
) -> Router<S> {
    Router::new()
        .nest("/.well-known", well_known::create_router(config))
        .nest("/xrpc", xrpc::create_router(config, algos))
        .nest("/stream", stream::create_router())
}
