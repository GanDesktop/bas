//! ACP JSON-RPC over a Bas-prepared gRPC connection.
//! The transport carries no Bas events, receipts or execution plans.
mod transport;
pub use transport::{Prepared, Transport, prepare};

pub use agent_client_protocol as sdk;
mod link;
pub use link::{Incoming, Link};
