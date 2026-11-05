//! Client connections and queries over the shared BAS wire contract.
pub use bas_protocol::connection::*;
pub use bas_protocol::protocol::*;
pub mod acp;
mod client;
pub use client::*;
