//! Owns running listeners and background tasks through graceful shutdown.

use super::Server;
use crate::{config, management::Manager, session::Registry, system};
use anyhow::{Context, Result};
use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

pub(super) struct Running {
    registry: Arc<Registry>,
    manager: Option<Arc<Manager>>,
    listeners: JoinSet<Result<(), tonic::transport::Error>>,
    maintenance: Option<AbortOnDropHandle<()>>,
    catalog_watcher: Option<AbortOnDropHandle<()>>,
    watchdog: Option<AbortOnDropHandle<()>>,
    stopping: CancellationToken,
    shutdown_grace: Duration,
}

impl Running {
    pub(super) fn start(server: Server, stopping: CancellationToken) -> Self {
        let Server {
            config,
            registry,
            manager,
            listeners,
        } = server;
        let maintenance = config.acp_maintenance.map(|interval| {
            AbortOnDropHandle::new(tokio::spawn(Arc::clone(&registry).maintain(interval)))
        });
        let catalog_watcher = config.catalog_source.map(|source| {
            AbortOnDropHandle::new(config::watch_catalog(
                source,
                config.agents,
                Arc::clone(&registry),
                stopping.clone(),
            ))
        });
        let watchdog = system::spawn_watchdog();
        system::ready();
        let mut tasks = JoinSet::new();
        for listener in listeners {
            ::tracing::info!(address = %listener.target(), "serving the bas.v1.Bas service");
            tasks.spawn(listener.serve(stopping.clone()));
        }
        Self {
            registry,
            manager,
            listeners: tasks,
            maintenance,
            catalog_watcher,
            watchdog,
            stopping,
            shutdown_grace: config.shutdown_grace,
        }
    }

    /// # Errors
    /// Returns listener, task, or bounded shutdown failures.
    pub(super) async fn serve(mut self) -> Result<()> {
        let listener_result = self.wait().await;
        system::stopping();
        self.stopping.cancel();
        let result =
            tokio::time::timeout(self.shutdown_grace, self.shutdown(listener_result)).await;
        if result.is_err() {
            self.listeners.abort_all();
            while self.listeners.join_next().await.is_some() {}
        }
        if let Some(watchdog) = self.watchdog.take() {
            watchdog.abort();
            let _ = watchdog.await;
        }
        result.context("shutdown grace expired")?
    }

    /// # Errors
    /// Reports the first listener failure or panic while observing cancellation.
    async fn wait(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                result = self.listeners.join_next() => match result {
                    Some(result) => result.context("listener task failed")?
                        .context("the gRPC server failed")?,
                    None => return Ok(()),
                },
                () = self.stopping.cancelled() => return Ok(()),
            }
        }
    }

    /// # Errors
    /// Preserves listener errors and reports failures while joining background tasks.
    async fn shutdown(&mut self, listener_result: Result<()>) -> Result<()> {
        let listeners = &mut self.listeners;
        let finish_listeners = async {
            let mut result = listener_result;
            // Drain every listener after cancellation even when one has already failed.
            while let Some(task) = listeners.join_next().await {
                result = result.and(
                    task.context("listener task failed")
                        .and_then(|result| result.context("the gRPC server failed")),
                );
            }
            result
        };
        let stop_management = async {
            if let Some(manager) = &self.manager {
                manager.shutdown().await;
            }
        };
        let (_, _, result) =
            tokio::join!(stop_management, self.registry.shutdown(), finish_listeners,);
        if let Some(maintenance) = self.maintenance.take() {
            maintenance.await.context("backend maintenance failed")?;
        }
        if let Some(catalog_watcher) = self.catalog_watcher.take() {
            catalog_watcher.await.context("catalog watcher failed")?;
        }
        result
    }
}
