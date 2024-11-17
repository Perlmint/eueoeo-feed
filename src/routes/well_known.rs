use crate::config::Config;
use axum::{response::IntoResponse, routing::get, Json, Router};

pub fn create_router<S: Clone + Send + Sync + 'static>(config: &Config) -> Router<S> {
    let service_endpoint = format!("https://{}", config.host_name);
    let did = Json(serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
        ],
        "id": config.service_did,
        "service": [
            {
                "id": "#bsky_fg",
                "type": "BskyFeedGenerator",
                "serviceEndpoint": service_endpoint,
            },
        ],
    }));

    Router::new().route(
        "/did.json",
        get(move || {
            let did = did.clone();
            async move { did.into_response() }
        }),
    )
}
