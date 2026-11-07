//! Typed installation progress and cancellation shared by acquisition steps.
use super::Error;
use anyhow::Result;
use bas_protocol::{management::Phase, protocol::pb::AgentOperation};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(super) struct Progress {
    pub(super) sender: Option<mpsc::Sender<AgentOperation>>,
    pub(super) cancellation: CancellationToken,
}
impl Progress {
    /// # Errors
    /// Returns an error if installation or the progress stream has been cancelled.
    pub(super) async fn send(&self, phase: Phase) -> Result<()> {
        self.check()?;
        if let Some(sender) = &self.sender {
            tokio::select! {
                biased;
                _ = self.closed() => return Err(Error::Cancelled.into()),
                result = sender.send(AgentOperation {
                    phase: phase.into(),
                    spec_json: Vec::new(),
                    complete: false,
                }) => result.map_err(|_| Error::Cancelled)?,
            }
        }
        Ok(())
    }
    pub(super) async fn closed(&self) {
        tokio::select! {
            _ = self.cancellation.cancelled() => {},
            _ = async {
                if let Some(sender) = &self.sender {
                    sender.closed().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {},
        }
    }
    pub(super) fn check(&self) -> Result<(), Error> {
        if self.cancellation.is_cancelled()
            || self.sender.as_ref().is_some_and(mpsc::Sender::is_closed)
        {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
