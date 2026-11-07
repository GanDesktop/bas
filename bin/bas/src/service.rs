//! Daemon RPC adapter over session, workspace, and agent-management contexts.

mod agents;
mod authentication;
mod authentication_error;
mod callbacks;
mod configuration;
mod history;
mod observation;
mod pagination;
mod prompts;
mod rpc;
mod sessions;
mod status;
mod workspaces;

use crate::session::{Registry, Session};
use bas_protocol::{
    model::SessionId,
    protocol::pb::{
        self,
        bas_server::{Bas, BasServer},
    },
};
use futures::Stream;
use std::{pin::Pin, str::FromStr, sync::Arc};
use tonic::{Request, Response, Status};

type EventStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send + 'static>>;

/// Implements `bas.v1.Bas` on top of a [`Registry`].
#[derive(Clone)]
pub struct Service {
    registry: Arc<Registry>,
    providers: Arc<crate::workspace::Providers>,
    manager: Option<Arc<crate::management::Manager>>,
}

impl Service {
    /// Serves the sessions held by `registry`.
    #[must_use]
    pub fn new(registry: Arc<Registry>) -> Self {
        Self::with_workspace_providers(registry, crate::workspace::Providers::default())
    }

    pub fn with_workspace_providers(
        registry: Arc<Registry>,
        providers: crate::workspace::Providers,
    ) -> Self {
        Self {
            registry,
            providers: Arc::new(providers),
            manager: None,
        }
    }

    pub fn with_manager(mut self, manager: Arc<crate::management::Manager>) -> Self {
        self.manager = Some(manager);
        self
    }

    fn manager(&self) -> Result<Arc<crate::management::Manager>, Status> {
        self.manager
            .clone()
            .ok_or_else(|| Status::unimplemented("Agent management is unavailable"))
    }

    /// Wraps the service for `tonic`'s server builder.
    #[must_use]
    pub fn into_server(self) -> BasServer<Self> {
        BasServer::new(self)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip)
            .max_decoding_message_size(bas_protocol::protocol::MAX_MESSAGE_SIZE)
            .max_encoding_message_size(bas_protocol::protocol::MAX_MESSAGE_SIZE)
    }

    async fn session(&self, id: &str) -> Result<Arc<Session>, Status> {
        let id =
            SessionId::from_str(id).map_err(|error| Status::invalid_argument(error.to_string()))?;

        self.registry
            .get(id)
            .await
            .map_err(|error| Status::not_found(error.to_string()))
    }
}

fn decode<T: serde::de::DeserializeOwned>(request: Request<pb::AcpPayload>) -> Result<T, Status> {
    request
        .into_inner()
        .decode()
        .map_err(|error| Status::invalid_argument(error.to_string()))
}
