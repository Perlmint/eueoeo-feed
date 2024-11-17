use std::{
    io::{Cursor, Read},
    net::ToSocketAddrs,
    sync::Arc,
};

use anyhow::{anyhow, Context};
use axum::Extension;
use clap::Parser;
use config::Config;
use log::{error, info};

mod algos;
mod atproto_subscription;
mod config;
mod data;
mod lexicon;
mod routes;
mod subscription;

use atproto_subscription::FirehoseSubscription;
use subscription::{ServiceSubscriptionHandler, UserProfile};

#[derive(Parser, Debug)]
enum Args {
    Run,
    Login,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let config_reader = std::fs::File::open("config.json")
        .map(|r| Box::new(r) as Box<dyn Read>)
        .unwrap_or_else(|e| {
            error!("Failed to open config.json - {e}");
            info!("Use fallback default config");
            Box::new(Cursor::new("{}"))
        });

    let config: Config =
        serde_json::from_reader(config_reader).context("Failed to parse config.json")?;

    let db_pool = sqlx::SqlitePool::connect(&config.sqlite_db).await?;
    sqlx::migrate!().run(&db_pool).await?;
    info!("DB migration completed");

    if let Args::Login = args {
        // TODO: login and save key
        return Ok(());
    }

    let (sender, receiver) = crossbeam::channel::bounded::<UserProfile>(30);

    let subscription = FirehoseSubscription::new(
        db_pool.clone(),
        config.subscription_endpoint.clone(),
        ServiceSubscriptionHandler::new(db_pool.clone(), sender),
    )
    .await?;
    let subscription_join = subscription.run()?;

    let listener = tokio::net::TcpListener::bind(
        &((config.listen_host.as_str(), config.port)
            .to_socket_addrs()
            .map_err(|_| anyhow!("Not a valid listen_host/port"))?
            .next()
            .unwrap()),
    )
    .await?;

    let algos = algos::create();

    let router = routes::create_router(&config, algos);
    let app = router
        .layer(Extension(db_pool))
        .layer(Extension(receiver))
        .layer(Extension(Arc::new(config)));
    let server = axum::serve(listener, app.into_make_service());

    let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();

    tokio::task::spawn(async move {
        let sig_int = tokio::signal::ctrl_c();
        #[cfg(target_family = "windows")]
        {
            sig_int.await.expect("Ctrl-C receiver is broken");
        }
        #[cfg(target_family = "unix")]
        {
            let mut sig_term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Failed to register SIGTERM handler");
            tokio::select! {
                _ = sig_int => (),
                _ = sig_term.recv() => (),
            };
        }

        if stop_sender.send(()).is_err() {
            error!("Already all services are stopped");
        }
    });

    server
        .with_graceful_shutdown(async move {
            let _ = stop_receiver.await;
        })
        .await?;

    subscription_join.await??;

    Ok(())
}
