//! Prepared daemon context and its running lifecycle.

mod handle;
mod listener;
mod runtime;

pub use handle::Handle;

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;

use crate::{
    agent::{Catalog, LocalHost},
    config::Config,
    management, service,
    session::Registry,
    storage,
};
use listener::Listener;

pub struct Server {
    config: Config,
    registry: Arc<Registry>,
    manager: Option<Arc<management::Manager>>,
    listeners: Vec<Listener>,
}

impl Server {
    /// Prepares storage, services, and bound listeners before starting the daemon.
    ///
    /// # Errors
    /// Returns an error if configuration, storage recovery, or listener setup fails.
    pub async fn new(config: Config) -> Result<Self> {
        config.validate()?;
        let storage: Arc<dyn storage::Storage> = match config.localdb_path()? {
            Some(path) => Arc::new(
                storage::LocalDb::open(&path)
                    .await
                    .with_context(|| format!("cannot open session database {}", path.display()))?,
            ),
            None => Arc::new(storage::Memory::new(config.limits.history_capacity)),
        };
        let mut registry = Registry::with_storage(
            Arc::new(Catalog::from_registrations(config.agents.clone())?),
            Arc::new(LocalHost::new(config.permissions)),
            config.limits,
            storage,
        );
        registry.set_acp_policy(config.acp.clone())?;
        registry
            .recover()
            .await
            .context("cannot recover stored sessions")?;
        let registry = Arc::new(registry);
        let manager = config
            .agent_management
            .then(|| {
                management::Manager::new(
                    config
                        .catalog_source
                        .as_ref()
                        .and_then(|source| source.settings_store()),
                )
                .map(Arc::new)
            })
            .transpose()?;
        let mut service = service::Service::new(Arc::clone(&registry));
        if let Some(manager) = &manager {
            service = service.with_manager(Arc::clone(manager));
        }
        let gateway =
            bas_server::Gateway::new(service.clone()).with_authentication(service.clone());
        let tls = config.tls.as_ref().map(|tls| tls.load()).transpose()?;
        let mut listeners = Vec::new();
        for target in &config.listen {
            listeners.push(Listener::bind(target, tls.clone(), &service, &gateway).await?);
        }
        Ok(Self {
            config,
            registry,
            manager,
            listeners,
        })
    }

    /// Starts the prepared daemon and returns control over its lifetime.
    pub fn run(self) -> Handle {
        let stopping = CancellationToken::new();
        let task = tokio::spawn(runtime::Running::start(self, stopping.clone()).serve());
        Handle::new(stopping, task)
    }
}
