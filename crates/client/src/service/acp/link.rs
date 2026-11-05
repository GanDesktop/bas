//! A running SDK connection; Bas controls stay outside the JSON-RPC transport.
use super::{Prepared, sdk};
use tokio::sync::{mpsc, oneshot};

pub enum Incoming {
    Request(sdk::UntypedMessage, sdk::Responder),
    Notification(sdk::UntypedMessage),
}

pub struct Link {
    pub peer: sdk::ConnectionTo<sdk::Agent>,
    pub prepared: Prepared,
    task: tokio::task::JoinHandle<sdk::Result<()>>,
}
impl Drop for Link {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Link {
    /// # Errors
    /// Returns transport or SDK startup errors.
    pub async fn open(
        mut prepared: Prepared,
    ) -> Result<(Self, mpsc::Receiver<Incoming>), tonic::Status> {
        let transport = prepared.connect().await?;
        let (events, receive) = mpsc::channel(64);
        let notifications = events.clone();
        let (ready, connected) = oneshot::channel();
        let task = tokio::spawn(async move {
            sdk::Client
                .builder()
                .on_receive_request(
                    async move |message: sdk::UntypedMessage, responder, _cx| {
                        events
                            .send(Incoming::Request(message, responder))
                            .await
                            .map_err(|_| sdk::Error::internal_error())
                    },
                    sdk::on_receive_request!(),
                )
                .on_receive_notification(
                    async move |message: sdk::UntypedMessage, _cx| {
                        notifications
                            .send(Incoming::Notification(message))
                            .await
                            .map_err(|_| sdk::Error::internal_error())
                    },
                    sdk::on_receive_notification!(),
                )
                .connect_with(transport, async move |peer| {
                    let _ = ready.send(peer.clone());
                    peer.incoming_closed().await;
                    Ok(())
                })
                .await
        });
        let peer = connected
            .await
            .map_err(|_| tonic::Status::unavailable("ACP connection closed during startup"))?;
        Ok((
            Self {
                peer,
                prepared,
                task,
            },
            receive,
        ))
    }
    pub fn is_closed(&self) -> bool {
        self.task.is_finished() || self.peer.is_incoming_closed()
    }
}
