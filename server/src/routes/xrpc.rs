use std::sync::Arc;

use axum::{
    body::HttpBody,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use log::error;
use sqlx::SqlitePool;

use crate::{
    algos::{AlgoHandlers, Context},
    config::Config,
    lexicon::{AtUri, app::bsky::feed::get_feed_skeleton},
};

async fn feed_generation(
    Extension(db): Extension<SqlitePool>,
    Extension(config): Extension<Arc<Config>>,
    Extension(algos): Extension<Arc<AlgoHandlers>>,
    Query(params): Query<get_feed_skeleton::QueryParams>,
) -> Response {
    let Ok(feed_uri): Result<AtUri, _> = params.feed.parse() else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "InvalidRequest",
            "message": "Error: feed must be a valid at-uri",
        }))).into_response();
    };

    if let (true, true, Some(algo)) = (
        feed_uri.authority == config.publisher_did,
        feed_uri
            .collection
            .map(|c| c == "app.bsky.feed.generator")
            .unwrap_or_default(),
        feed_uri.rkey.and_then(|name| algos.get(&name)),
    ) {
        match algo.handle(Context { db, config }, params).await {
            Ok(body) => (
                StatusCode::OK,
                Json(serde_json::json!(body)),
            ),
            Err(e) => {
                error!("Failed to generate feed - {e:?}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "InternalServerError",
                        "message": "Error: Internal server error",
                    })),
                )
            }
        }
        .into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "UnsupportedAlgorithm",
                "message": "Error: Unsupported algorithm",
            })),
        )
            .into_response()
    }
}

pub fn create_router<B: HttpBody + Send + 'static, S: Clone + Send + Sync + 'static>(
    config: &Config,
    algos: AlgoHandlers,
) -> Router<S, B> {
    let feeds = algos
        .keys()
        .map(|shortname| {
            let uri =
                AtUri::new(config.publisher_did.clone(), Some(shortname.clone()), None).to_string();
            serde_json::json!({ "uri": uri })
        })
        .collect::<Vec<_>>();
    let description = Json(serde_json::json!({
        "encoding": "application/json",
        "body": {
            "did": config.service_did,
            "feeds": feeds,
        }
    }));
    let algos = Arc::new(algos);

    Router::new()
        .route("/app.bsky.feed.getFeedSkeleton", get(feed_generation))
        .layer(Extension(algos))
        .route(
            "/app.bsky.feed.describeFeedGenerator",
            get(move || {
                let description = description.clone();
                async move { description.into_response() }
            }),
        )
}
