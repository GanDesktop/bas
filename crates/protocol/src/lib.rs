//! Shared BAS data and wire contracts. No daemon or frontend implementation.
//! Connection I/O and generated RPC stubs are enabled explicitly by each role.
pub mod authentication;
pub mod config;
#[cfg(feature = "transport")]
pub mod connection;
pub mod management;
pub mod model;
pub mod protocol;

pub use agent_client_protocol_schema::v1 as acp;
pub use agent_client_protocol_schema::v2 as acp_v2;

pub mod workspace;
