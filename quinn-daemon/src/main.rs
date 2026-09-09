mod commands;
mod service;
mod tools;

use std::{env, error::Error, path::PathBuf, sync::Arc};

use needle_infer::v2_engine::V2Engine;
use tracing::info;
use tracing_subscriber::EnvFilter;
use zbus::connection;

const DEFAULT_MODEL_RELATIVE: &str = ".local/share/quinn/weights/needle2.cact";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("quinn_daemon=info")),
        )
        .init();

    let model_path = model_path();
    info!(path = %model_path.display(), "loading Needle v2");
    let engine = Arc::new(V2Engine::load(&model_path)?);
    info!("Needle v2 ready");

    let daemon = service::QuinnDaemon::new(engine);
    let _connection = connection::Builder::session()?
        .name("org.quinn.Assistant")?
        .serve_at("/org/quinn/Assistant", daemon)?
        .build()
        .await?;

    info!("Quinn D-Bus service ready");
    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn model_path() -> PathBuf {
    if let Ok(path) = env::var("QUINN_MODEL") {
        return PathBuf::from(path);
    }

    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(DEFAULT_MODEL_RELATIVE)
}
