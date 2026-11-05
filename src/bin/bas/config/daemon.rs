use std::path::PathBuf;

use bas::{config::AgentRegistration, service::Target};

use crate::{
    agent::{AgentSpec, PermissionPolicy},
    session::Limits,
};

/// Semicolon-separated transport endpoints the gRPC service listens on.
pub const LISTEN_ENV: &str = "BAS_LISTEN";
/// Comma-free, semicolon-separated agent catalog. See [`super::parse_agents`].
pub const AGENTS_ENV: &str = "BAS_AGENTS";
/// Replayable events kept per session.
pub const HISTORY_ENV: &str = "BAS_HISTORY";
/// Storage backend: `memory` or `localdb`.
pub const STORAGE_ENV: &str = "BAS_STORAGE";
/// SQLite file path when using `localdb`.
pub const LOCALDB_PATH_ENV: &str = "BAS_LOCALDB_PATH";

pub const BUFFER_ENV: &str = "BAS_BUFFER";
pub const STARTUP_TIMEOUT_ENV: &str = "BAS_STARTUP_TIMEOUT";
pub const PERMISSIONS_ENV: &str = "BAS_PERMISSIONS";

/// Storage for session metadata and replayable events.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub enum Storage {
    /// Keep sessions only until the daemon stops.
    Memory,
    /// Persist sessions to a local SQLite database.
    #[default]
    Localdb,
}

/// Daemon configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub listen: Vec<Target>,
    pub tls: Option<bas::service::tls::ServerTls>,
    pub agents: Vec<AgentRegistration>,
    pub acp: bas::config::AcpLifecycle,
    pub catalog_source: Option<super::CatalogSource>,
    pub limits: Limits,
    pub permissions: PermissionPolicy,
    pub storage: Storage,
    pub localdb_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: vec![Target::Local],
            tls: None,
            agents: vec![AgentSpec::embedded().into()],
            acp: bas::config::AcpLifecycle::default(),
            catalog_source: None,
            limits: Limits::default(),
            permissions: PermissionPolicy::default(),
            storage: Storage::default(),
            localdb_path: None,
        }
    }
}

impl Config {
    /// Resolves the SQLite file only when local persistence is selected.
    ///
    /// # Errors
    /// Returns an error when neither an explicit location nor a usable user state
    /// directory can be found.
    pub fn localdb_path(&self) -> anyhow::Result<Option<PathBuf>> {
        if self.storage == Storage::Memory {
            return Ok(None);
        }
        if let Some(path) = &self.localdb_path {
            return Ok(Some(path.clone()));
        }
        let state = bas::config::paths::state_root()
            .ok_or_else(|| anyhow::anyhow!("set --localdb-path or a user state directory"))?;
        Ok(Some(state.join("bas/sessions.sqlite3")))
    }
}
