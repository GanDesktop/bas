use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Controls a running server without coupling it to process signals.
/// Dropping the handle requests graceful shutdown.
pub struct Handle {
    stopping: CancellationToken,
    task: JoinHandle<Result<()>>,
}

impl Handle {
    pub(super) fn new(stopping: CancellationToken, task: JoinHandle<Result<()>>) -> Self {
        Self { stopping, task }
    }

    pub fn cancel(&self) {
        self.stopping.cancel();
    }

    /// Waits for the server and its cleanup to finish. Safe to use in select!.
    ///
    /// # Errors
    /// Returns server, shutdown, or task failures.
    pub async fn wait(&mut self) -> Result<()> {
        (&mut self.task).await.context("server task failed")?
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.cancel();
    }
}
