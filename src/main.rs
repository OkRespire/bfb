mod app;
mod dirview;
mod fs;
mod ui;

use color_eyre::Result;
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt};

use crate::app::App;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let _guard = init_logging();
    tracing::info!("bfb starting up");

    let terminal = ratatui::init();
    let result = App::new().await?.run(terminal).await;
    ratatui::restore();
    result
}

fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let file_appender = rolling::never(".", "bfb.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    fmt()
        .with_writer(non_blocking)
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(false)
        .init();

    guard
}
