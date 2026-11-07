mod cli;
mod demo;
mod extensions;
mod v1;
mod v2;

use agent_client_protocol::{Agent, ConnectTo, Stdio};

use crate::{cli::Cli, demo::Options};
use clap::Parser;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.options()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // stdout belongs to the JSON-RPC transport; diagnostics go to stderr.
            eprintln!("acp-test-agent: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Serve one ACP connection over stdio, routing v1 and v2 clients to their
/// respective implementations.
///
/// # Errors
///
/// Returns an error when the transport fails or when a protocol implementation
/// stops with an error.
async fn run(options: Options) -> agent_client_protocol::Result<()> {
    let mut router = Agent.protocol_router();
    if options.v1 {
        router = router.with_v1(v1::component(options.clone()));
    }
    if options.v2 {
        router = router.with_v2(v2::component(options.clone()));
    }
    router.connect_to(Stdio::new()).await
}
