//! Native ACP authentication and logout across unary and interactive transports.
use super::*;
use crate::agent::AgentSpec;
use crate::session::CreateError;
use futures::StreamExt;

impl Service {
    pub(super) async fn handle_authenticate_agent_interactive(
        &self,
        request: Request<tonic::Streaming<pb::AuthenticationInput>>,
    ) -> Result<Response<EventStream<pb::AuthenticationEvent>>, Status> {
        self.authentication_stream(Box::pin(request.into_inner()))
            .await
            .map(Response::new)
    }

    pub(super) async fn handle_logout_agent(
        &self,
        request: Request<pb::LogoutAgentRequest>,
    ) -> Result<Response<pb::LogoutAgentResponse>, Status> {
        let request = request.into_inner();
        let expected = expected_spec(&request.expected_spec_json)?;
        let catalog = self.registry.catalog();
        let spec = catalog
            .spec(&request.agent.into())
            .filter(|spec| spec.as_ref() == &expected)
            .ok_or_else(|| Status::failed_precondition("Agent configuration changed; retry"))?;
        if spec.kind != crate::agent::Kind::Acp {
            return Err(Status::invalid_argument("Only ACP agents support logout"));
        }
        catalog
            .validate(spec)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        let _credentials = self.registry.credentials_change(spec.id.clone());
        tokio::select! {
            result = tokio::time::timeout(self.registry.startup_timeout(), crate::agent::acp::logout(spec)) => {
                result.map_err(|_| Status::deadline_exceeded("Logout timed out; check authentication state before retrying"))?
                    .map_err(CreateError::Agent)?;
            }
            () = self.registry.stopped() => return Err(Status::unavailable("Daemon is stopping")),
        }
        Ok(Response::new(pb::LogoutAgentResponse {}))
    }

    pub(super) async fn handle_authenticate_agent(
        &self,
        request: Request<pb::AuthenticateAgentRequest>,
    ) -> Result<Response<pb::AuthenticateAgentResponse>, Status> {
        let request = request.into_inner();
        let expected = expected_spec(&request.expected_spec_json)?;
        let catalog = self.registry.catalog();
        let spec = catalog
            .spec(&request.agent.into())
            .filter(|spec| spec.as_ref() == &expected)
            .ok_or_else(|| Status::failed_precondition("Agent configuration changed; retry"))?;
        if spec.kind != crate::agent::Kind::Acp {
            return Err(Status::invalid_argument(
                "Only ACP agents support authentication",
            ));
        }
        catalog
            .validate(spec)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        let program = crate::agent::acp::resolve_program(spec)
            .ok_or_else(|| Status::failed_precondition("Agent executable is missing"))?;
        let expected_method = if request.expected_method_json.is_empty() {
            None
        } else {
            Some(
                serde_json::from_slice(&request.expected_method_json).map_err(|_| {
                    Status::invalid_argument(
                        "expected_method_json must contain a method descriptor",
                    )
                })?,
            )
        };
        let _credentials = self.registry.credentials_change(spec.id.clone());
        let result = tokio::select! {
            result = crate::agent::acp::authenticate_expected(spec, &request.method_id, expected_method, std::time::Duration::from_secs(300)) => result,
            () = self.registry.stopped() => return Err(Status::unavailable("Daemon is stopping")),
        }.map_err(|error| super::authentication_error::status(&error))?;
        if !Arc::ptr_eq(&catalog, &self.registry.catalog()) {
            return Err(Status::failed_precondition(
                "Agent catalog changed during authentication; retry",
            ));
        }
        Ok(Response::new(pb::AuthenticateAgentResponse {
            check: Some(authentication_check(
                result,
                &program,
                "Authentication flow completed; session readiness checked",
            )),
        }))
    }
}

fn authentication_check(
    result: crate::agent::acp::CheckResult,
    program: &std::path::Path,
    message: &str,
) -> pb::CheckAgentResponse {
    let (status, initialized, authentication_error) = match result {
        crate::agent::acp::CheckResult::Connected(initialized) => {
            (pb::AgentCheckStatus::Connected, initialized, None)
        }
        crate::agent::acp::CheckResult::AuthenticationRequired(initialized, error) => (
            pb::AgentCheckStatus::AuthenticationRequired,
            initialized,
            Some((&error).into()),
        ),
    };
    pb::CheckAgentResponse {
        status: status.into(),
        message: message.into(),
        resolved_program: program.to_string_lossy().into_owned(),
        initialization: Some((&initialized).into()),
        authentication_error,
    }
}

/// # Errors
/// Rejects requests without a serialized saved agent specification.
pub(super) fn expected_spec(bytes: &[u8]) -> Result<AgentSpec, Status> {
    serde_json::from_slice(bytes)
        .map_err(|_| Status::invalid_argument("expected_spec_json must contain a saved AgentSpec"))
}

impl Service {
    async fn authentication_stream(
        &self,
        input: bas_server::AuthenticationInput,
    ) -> Result<bas_server::AuthenticationEvents, Status> {
        let mut input = input;
        let first = tokio::time::timeout(std::time::Duration::from_secs(30), input.next())
            .await
            .map_err(|_| Status::deadline_exceeded("Authentication start was not received"))?
            .ok_or_else(|| Status::invalid_argument("Authentication requires a start message"))??;
        let Some(pb::authentication_input::Kind::Start(start)) = first.kind else {
            return Err(Status::invalid_argument(
                "The first authentication message must be start",
            ));
        };
        let expected = expected_spec(&start.expected_spec_json)?;
        let catalog = self.registry.catalog();
        let spec = catalog
            .spec(&start.agent.clone().into())
            .filter(|spec| spec.as_ref() == &expected)
            .cloned()
            .ok_or_else(|| Status::failed_precondition("Agent configuration changed; retry"))?;
        if spec.kind != crate::agent::Kind::Acp {
            return Err(Status::invalid_argument(
                "Only ACP agents support authentication",
            ));
        }
        catalog
            .validate(&spec)
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        let program = crate::agent::acp::resolve_program(&spec)
            .ok_or_else(|| Status::failed_precondition("Agent executable is missing"))?;
        let managed = catalog.managed(&spec.id).cloned();
        let registry = Arc::clone(&self.registry);
        let (send, receive) = tokio::sync::mpsc::channel(32);
        let task = tokio::spawn(async move {
            let _credentials = registry.credentials_change(spec.id.clone());
            let (controls, controls_receive) = tokio::sync::mpsc::channel(16);
            let operation = crate::agent::acp::authenticate_interactive(
                &spec,
                managed.as_ref(),
                &start,
                send.clone(),
                controls_receive,
                std::time::Duration::from_secs(900),
            );
            tokio::pin!(operation);
            let result = loop {
                tokio::select! {
                    biased;
                    () = send.closed() => return,
                    () = registry.stopped() => break Err(Status::unavailable("Daemon is stopping")),
                    result = &mut operation => break result.map_err(|error| super::authentication_error::status(&error)),
                    message = input.next() => match message {
                        Some(Ok(message)) => {
                            if !Arc::ptr_eq(&catalog, &registry.catalog()) {
                                break Err(Status::failed_precondition("Agent catalog changed during authentication; retry"));
                            }
                            if controls.send(message).await.is_err() { break Err(Status::cancelled("Authentication flow closed")); }
                        }
                        None => break Err(Status::cancelled("Authentication input closed")),
                        Some(Err(error)) => break Err(error),
                    },
                }
            };
            let result = result.and_then(|result| {
                if !Arc::ptr_eq(&catalog, &registry.catalog()) {
                    return Err(Status::failed_precondition(
                        "Agent catalog changed during authentication; retry",
                    ));
                }
                Ok(pb::AuthenticationEvent {
                    kind: Some(pb::authentication_event::Kind::Check(authentication_check(
                        result,
                        &program,
                        "Agent setup completed; readiness checked",
                    ))),
                })
            });
            let _ = send.send(result).await;
        });
        let stream = futures::stream::unfold(
            (receive, tokio_util::task::AbortOnDropHandle::new(task)),
            |(mut receive, task)| async move {
                receive.recv().await.map(|event| (event, (receive, task)))
            },
        );
        Ok(Box::pin(stream) as bas_server::AuthenticationEvents)
    }
}
impl bas_server::Authentication for Service {
    fn start(
        &self,
        input: bas_server::AuthenticationInput,
    ) -> futures::future::BoxFuture<'static, Result<bas_server::AuthenticationEvents, Status>> {
        let service = self.clone();
        Box::pin(async move { service.authentication_stream(input).await })
    }
}
