//! Daemon settings with conflict-aware persistence.
mod acp;
pub mod agent;
pub mod paths;
pub mod registration;
mod store;
pub use acp::{AcpLifecycle, MAX_ACP_POOL_RESERVE};
pub use agent::{AgentSpec, validate_agents};
pub use registration::*;
use serde::{Deserialize, Serialize};
pub use store::{DaemonDocument, DaemonStore, Document, Error, Store};

/// Legacy unified representation for imports and compatibility tools.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub daemon: Daemon,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Daemon {
    pub agents: Vec<AgentRegistration>,
    pub acp: AcpLifecycle,
}
impl Daemon {
    /// Validates registrations and their ACP policy without reading installations.
    ///
    /// # Errors
    /// Rejects invalid registrations before checking policy references and reserves.
    pub fn validate(&self) -> Result<(), RegistrationError> {
        validate_registrations(&self.agents)?;
        self.acp.validate(&self.agents)?;
        Ok(())
    }
}
impl Default for Daemon {
    fn default() -> Self {
        Self {
            agents: vec![AgentSpec::embedded().into()],
            acp: AcpLifecycle::default(),
        }
    }
}
#[cfg(test)]
mod tests;
