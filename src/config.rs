//! Daemon settings with conflict-aware persistence.
mod store;
pub use bas_protocol::config::{
    AcpLifecycle, AgentSpec, Daemon, MAX_ACP_POOL_RESERVE, agent, paths, registration,
    validate_agents,
};
pub use registration::*;
use serde::{Deserialize, Serialize};
pub use store::{DaemonDocument, DaemonStore, Document, Error, Store};

/// Legacy unified representation for imports and compatibility tools.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub daemon: Daemon,
}
#[cfg(test)]
mod tests;
