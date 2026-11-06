//! Connection liveness uses HTTP/2 PING/ACK, independently of RPC activity.
use std::time::Duration;
use tonic::transport::Endpoint;

pub const INTERVAL: Duration = Duration::from_secs(15);
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Also probe channels with no active RPCs; ordinary RPC deadlines stay separate.
pub fn client(endpoint: Endpoint) -> Endpoint {
    endpoint
        .http2_keep_alive_interval(INTERVAL)
        .keep_alive_timeout(TIMEOUT)
        .keep_alive_while_idle(true)
}

/// Release dead peers and their streams without changing session ownership.
#[cfg(feature = "server")]
pub fn server<L>(server: tonic::transport::Server<L>) -> tonic::transport::Server<L> {
    server
        .http2_keepalive_interval(Some(INTERVAL))
        .http2_keepalive_timeout(Some(TIMEOUT))
}
