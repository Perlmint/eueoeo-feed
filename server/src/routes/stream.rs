use std::convert::Infallible;

use axum::{
    body::HttpBody,
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    routing::get,
    Extension, Router,
};
use crossbeam::channel::Receiver;
use futures_util::{stream, Stream};

pub fn create_router<B: HttpBody + Send + 'static, S: Clone + Send + Sync + 'static>(
) -> Router<S, B> {
    async fn sse_handler(
        Extension(channel): Extension<Receiver<String>>,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        Sse::new(stream::repeat_with(move || {
            Ok(Event::default().data(channel.recv().unwrap()))
        }))
        .keep_alive(KeepAlive::default())
    }

    Router::new().route("/", get(sse_handler))
}
