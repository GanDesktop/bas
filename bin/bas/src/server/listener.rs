use anyhow::{Context, Result};
use bas_protocol::connection::{Target, heartbeat, transport};
use bas_server::Gateway;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Server, ServerTlsConfig, server::Router};

use crate::service::Service;

pub(super) struct Listener {
    target: Target,
    transport: transport::Listener,
    router: Router,
}

impl Listener {
    /// # Errors
    /// Returns an error if TLS configuration or transport binding fails.
    pub(super) async fn bind(
        target: &Target,
        tls: Option<ServerTlsConfig>,
        service: &Service,
        gateway: &Gateway<Service>,
    ) -> Result<Self> {
        let mut server = heartbeat::server(Server::builder());
        let bind_target = match target {
            Target::Mtls(address) => {
                server = server
                    .tls_config(tls.context("mTLS listener requires server TLS configuration")?)?;
                Target::Tcp(address.clone())
            }
            _ => target.clone(),
        };
        let transport = transport::Listener::bind(&bind_target)
            .await
            .with_context(|| format!("cannot listen on {target}"))?;
        let target = match (target, transport.target()) {
            (Target::Mtls(_), Target::Tcp(address)) => Target::Mtls(address.clone()),
            _ => transport.target().clone(),
        };
        let router = server
            .add_service(service.clone().into_server())
            .add_service(gateway.clone().into_server());
        Ok(Self {
            target,
            transport,
            router,
        })
    }

    pub(super) fn target(&self) -> &Target {
        &self.target
    }

    /// # Errors
    /// Returns an error if the gRPC listener fails.
    pub(super) async fn serve(
        self,
        stopping: CancellationToken,
    ) -> Result<(), tonic::transport::Error> {
        self.router
            .serve_with_incoming_shutdown(self.transport.incoming(), stopping.cancelled_owned())
            .await
    }
}
