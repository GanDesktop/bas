//! Authenticated connection primitives shared by client connectors and listeners.
pub mod heartbeat;
mod target;
pub mod tls;
pub mod transport;
pub use crate::protocol::{DEFAULT_ADDRESS, DEFAULT_PORT, DEFAULT_TARGET};
pub use target::*;
