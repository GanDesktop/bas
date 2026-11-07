//! Agent discovery, installation, removal, and readiness checks.
use super::authentication::expected_spec;
use super::*;

impl Service {
    pub(super) async fn handle_list_agent_catalog(
        &self,
        request: Request<pb::ListAgentCatalogRequest>,
    ) -> Result<Response<pb::ListAgentCatalogResponse>, Status> {
        let manager = self.manager()?;
        let registrations = self.registry.catalog().registrations().cloned().collect();
        let catalog = manager
            .catalog(registrations, request.into_inner().refresh)
            .await
            .map_err(|error| Status::unavailable(error.to_string()))?;
        Ok(Response::new(pb::ListAgentCatalogResponse {
            catalog_json: serde_json::to_vec(&catalog)
                .map_err(|_| Status::internal("Could not encode agent catalog"))?,
        }))
    }

    pub(super) async fn handle_install_agent(
        &self,
        request: Request<pb::InstallAgentRequest>,
    ) -> Result<Response<EventStream<pb::AgentOperation>>, Status> {
        let manager = self.manager()?;
        let registry = Arc::clone(&self.registry);
        let id = request.into_inner().id;
        let (send, receive) = tokio::sync::mpsc::channel(16);
        let task = tokio::spawn(async move {
            let (progress, mut updates) = tokio::sync::mpsc::channel(16);
            let install = manager.install(&id, progress);
            tokio::pin!(install);
            let result = loop {
                tokio::select! {
                    biased;
                    () = registry.stopped() => break Err(anyhow::anyhow!("Daemon is stopping")),
                    () = send.closed() => return,
                    result = &mut install => break result,
                    Some(update) = updates.recv() => {
                        if send.send(Ok(update)).await.is_err() { return; }
                    }
                }
            };
            let result = result.and_then(|spec| {
                let store = manager
                    .store()
                    .ok_or_else(|| anyhow::anyhow!("No writable settings catalog"))?;
                registry.reload_catalog(|| Ok(store.load()?.settings.agents))?;
                Ok(spec)
            });
            let update = match result {
                Ok(spec) => serde_json::to_vec(&spec)
                    .map(|spec_json| pb::AgentOperation {
                        phase: bas_protocol::management::Phase::Complete.into(),
                        spec_json,
                        complete: true,
                    })
                    .map_err(|_| Status::internal("Could not encode installed agent")),
                Err(error) => Err(Status::failed_precondition(error.to_string())),
            };
            let _ = send.send(update).await;
        });
        let stream = futures::stream::unfold(
            (receive, tokio_util::task::AbortOnDropHandle::new(task)),
            |(mut receive, task)| async move {
                receive.recv().await.map(|update| (update, (receive, task)))
            },
        );
        Ok(Response::new(Box::pin(stream)))
    }

    pub(super) async fn handle_remove_agent(
        &self,
        request: Request<pb::RemoveAgentRequest>,
    ) -> Result<Response<pb::RemoveAgentResponse>, Status> {
        let manager = self.manager()?;
        manager
            .remove(&request.into_inner().id)
            .await
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        self.registry
            .reload_catalog(|| {
                Ok(manager
                    .store()
                    .ok_or_else(|| anyhow::anyhow!("No writable settings catalog"))?
                    .load()?
                    .settings
                    .agents)
            })
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Response::new(pb::RemoveAgentResponse {}))
    }

    pub(super) async fn handle_list_agents(
        &self,
        _request: Request<pb::ListAgentsRequest>,
    ) -> Result<Response<pb::ListAgentsResponse>, Status> {
        let agents = self
            .registry
            .catalog()
            .specs()
            .filter(|spec| {
                spec.kind == crate::agent::Kind::Embedded
                    || crate::agent::acp::resolve_program(spec).is_some()
            })
            .map(|spec| pb::Agent {
                id: spec.id.to_string(),
                kind: spec.kind.to_string(),
                command: spec.command().map(ToString::to_string).unwrap_or_default(),
            })
            .collect();

        Ok(Response::new(pb::ListAgentsResponse { agents }))
    }

    pub(super) async fn handle_check_agent(
        &self,
        request: Request<pb::CheckAgentRequest>,
    ) -> Result<Response<pb::CheckAgentResponse>, Status> {
        use crate::agent::{Kind, acp};
        use pb::AgentCheckStatus as Check;
        let request = request.into_inner();
        let expected = expected_spec(&request.expected_spec_json)?;
        let catalog = self.registry.catalog();
        let id = request.agent.into();
        let spec = catalog
            .spec(&id)
            .filter(|spec| spec.as_ref() == &expected)
            .ok_or_else(|| {
                Status::failed_precondition("Agent configuration changed; save and retry")
            })?;
        catalog
            .validate(spec)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        let mut response = pb::CheckAgentResponse {
            status: Check::Connected.into(),
            message: "Embedded agent is ready".into(),
            resolved_program: String::new(),
            initialization: None,
            authentication_error: None,
        };
        if spec.kind == Kind::Acp {
            if let Some(program) = acp::resolve_program(spec) {
                response.resolved_program = program.to_string_lossy().into_owned();
                response.status = Check::Installed.into();
                response.message =
                    "Executable is installed; connection and login have not been checked".into();
                if request.connect {
                    let result = tokio::select! {
                        result = acp::check_connection(spec, self.registry.startup_timeout()) => result,
                        () = self.registry.stopped() => return Err(Status::unavailable("Daemon is stopping")),
                    };
                    match result {
                        Ok(acp::CheckResult::Connected(initialized)) => {
                            response.status = Check::Connected.into();
                            response.message = "ACP initialize and newSession succeeded".into();
                            response.initialization = Some((&initialized).into());
                        }
                        Ok(acp::CheckResult::AuthenticationRequired(initialized, error)) => {
                            response.status = Check::AuthenticationRequired.into();
                            response.message = "Agent authentication required".into();
                            response.initialization = Some((&initialized).into());
                            response.authentication_error = Some((&error).into());
                        }
                        Err(message) => {
                            response.status = Check::Failed.into();
                            response.message = message.into();
                        }
                    }
                }
            } else {
                response.status = Check::MissingExecutable.into();
                response.message =
                    "Executable is missing or not executable on the daemon host".into();
            }
        }
        // A replaced catalog invalidates even an otherwise successful probe.
        if !Arc::ptr_eq(&catalog, &self.registry.catalog()) {
            return Err(Status::failed_precondition(
                "Agent catalog changed during check; retry",
            ));
        }
        Ok(Response::new(response))
    }
}
