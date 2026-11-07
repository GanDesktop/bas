//! `bas` — the Bas agent service daemon.
//!
//! The daemon owns agent sessions and exposes them over gRPC. It is built to run
//! under systemd: logs go to journald, readiness is signalled with `sd_notify`,
//! and `WatchdogSec=` is honored.

mod agent;
mod clap;
mod config;
mod management;
mod server;
mod service;
mod session;
mod storage;
mod system;
mod tracing;
mod workspace;

use ::clap::Parser;
use anyhow::{Context, Result};

use server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    let args = clap::Args::parse();
    let config = args.config()?;

    let tracing_backend = args
        .tracing()
        .with_syslog_identifier(tracing::DEFAULT_SYSLOG_IDENTIFIER)
        .init()?;

    ::tracing::info!(
        tracing_backend = %tracing_backend,
        listen = ?config.listen,
        agents = config.agents.len(),
        "bas starting"
    );

    let shutdown = system::shutdown_signal().context("cannot install shutdown handlers")?;
    let server = Server::new(config).await?;
    let mut handle = server.run();
    tokio::select! {
        result = handle.wait() => result,
        signal = shutdown => {
            handle.cancel();
            let result = handle.wait().await;
            signal.context("shutdown signal failed")?;
            result
        }
    }
}
