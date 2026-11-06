//! Authenticated BAS RPC/ACP clients. Shared data contracts live in `bas-protocol`.
pub use bas_protocol::{acp, acp_v2, authentication, config, management, model};
pub mod service;

pub use bas_protocol::workspace;
