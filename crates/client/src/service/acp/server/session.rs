use super::connection::{payload, protocol, status, value};
use super::*;
use agent_client_protocol::{Client, ConnectionTo, UntypedMessage};
use serde_json::{Value, json};

pub(super) fn validate(
    connection: &Connection,
    params: &Value,
) -> Result<String, crate::acp::Error> {
    let pb::prepare_acp_request::Target::Session(session) = &connection.target else {
        return Err(crate::acp::Error::invalid_params());
    };
    if params.get("sessionId").and_then(Value::as_str) != Some(session.as_str()) {
        return Err(crate::acp::Error::invalid_params());
    }
    Ok(session.clone())
}
pub(super) async fn load<B: Bas + 'static>(
    backend: Arc<B>,
    connection: Arc<Connection>,
    params: Value,
    cx: ConnectionTo<Client>,
) -> Result<Value, crate::acp::Error> {
    let session = validate(&connection, &params)?;
    let snapshot = backend
        .get_session(Request::new(pb::GetSessionRequest {
            session: session.clone(),
        }))
        .await
        .map_err(status)?
        .into_inner()
        .session
        .ok_or_else(crate::acp::Error::internal_error)?;
    let load: crate::acp::LoadSessionRequest =
        serde_json::from_value(params).map_err(|_| crate::acp::Error::invalid_params())?;
    if load.cwd
        != std::path::Path::new(
            snapshot
                .cwd
                .as_deref()
                .ok_or_else(crate::acp::Error::internal_error)?,
        )
        || !load.mcp_servers.is_empty()
    {
        return Err(crate::acp::Error::invalid_params()
            .data("configure the session through Bas before attaching"));
    }
    let mut after = None;
    loop {
        let page = backend
            .load_history(Request::new(pb::LoadHistoryRequest {
                session: session.clone(),
                after,
                limit: 256,
            }))
            .await
            .map_err(status)?
            .into_inner();
        for event in page
            .events
            .iter()
            .filter(|event| event.cursor <= snapshot.revision)
        {
            notify(&cx, event)?;
        }
        match page.next {
            Some(next)
                if next < snapshot.revision && after.is_none_or(|previous| next > previous) =>
            {
                after = Some(next)
            }
            Some(next) if after.is_some_and(|previous| next <= previous) => {
                return Err(protocol("non-advancing history cursor"));
            }
            _ => break,
        }
    }
    let events = backend
        .attach(Request::new(pb::AttachRequest {
            session: session.clone(),
            cursor: Some(snapshot.revision),
            replay: 0,
        }))
        .await
        .map_err(status)?
        .into_inner();
    let mut events = Box::pin(events);
    let live = cx.clone();
    cx.spawn(async move {
        while let Some(event) = events.next().await {
            notify(&live, &event.map_err(status)?)?;
        }
        Ok(())
    })?;
    if connection.interactive {
        callbacks(backend, connection, session, cx).await?;
    }
    let mut result = snapshot.acp.map(value).transpose()?.unwrap_or(json!({}));
    result
        .as_object_mut()
        .ok_or_else(crate::acp::Error::internal_error)?
        .remove("sessionId");
    Ok(result)
}
fn notify(cx: &ConnectionTo<Client>, event: &pb::Event) -> Result<(), crate::acp::Error> {
    use pb::event::Kind;
    match &event.kind {
        Some(Kind::AcpUpdate(update)) => {
            cx.send_notification(UntypedMessage::new(
                "session/update",
                json!({"sessionId": event.session, "update": value(update.clone())?}),
            )?)?;
        }
        Some(Kind::SubagentUpdate(notification)) => {
            cx.send_notification(UntypedMessage::new(
                "session/update",
                value(notification.clone())?,
            )?)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) async fn request<B: Bas + 'static>(
    backend: Arc<B>,
    connection: Arc<Connection>,
    request: UntypedMessage,
    request_id: String,
) -> Result<Value, crate::acp::Error> {
    let session = validate(&connection, &request.params)?;
    match request.method.as_str() {
        "session/prompt" => prompt(backend, connection, session, request.params, request_id).await,
        "session/set_config_option" => value(
            backend
                .set_config_option(Request::new(payload(request.params)))
                .await
                .map_err(status)?
                .into_inner(),
        ),
        "session/set_mode" => value(
            backend
                .set_mode(Request::new(payload(request.params)))
                .await
                .map_err(status)?
                .into_inner(),
        ),
        _ => Err(crate::acp::Error::method_not_found()),
    }
}
async fn prompt<B: Bas + 'static>(
    backend: Arc<B>,
    connection: Arc<Connection>,
    session: String,
    params: Value,
    request_id: String,
) -> Result<Value, crate::acp::Error> {
    let turn = connection
        .bindings
        .lock()
        .expect("submission bindings")
        .get(&request_id)
        .map(|binding| binding.turn.clone())
        .unwrap_or_else(|| crate::model::TurnId::new().to_string());
    // The backend commits and owns execution; transport cancellation cannot stop it.
    let response = backend
        .prompt(Request::new(pb::PromptSubmission {
            turn: turn.clone(),
            prompt: Some(payload(params)),
        }))
        .await
        .map(Response::into_inner);
    if let Some(binding) = connection
        .bindings
        .lock()
        .expect("submission bindings")
        .get_mut(&request_id)
    {
        binding.outcome = Some(response.clone());
    }
    connection.submitted.notify_waiters();
    let response = response.map_err(status)?;
    let cursor = match response.outcome {
        Some(pb::prompt_submission_response::Outcome::Accepted(accepted)) => accepted.cursor,
        Some(pb::prompt_submission_response::Outcome::Rejected(rejected)) => {
            return Err(protocol(rejected.message));
        }
        None => return Err(crate::acp::Error::internal_error()),
    };
    let events = backend
        .attach(Request::new(pb::AttachRequest {
            session,
            cursor: Some(cursor),
            replay: 0,
        }))
        .await
        .map_err(status)?
        .into_inner();
    let mut events = Box::pin(events);
    while let Some(event) = events.next().await {
        match event.map_err(status)?.kind {
            Some(pb::event::Kind::TurnFinished(finished)) if finished.turn == turn => {
                return match finished.outcome {
                    Some(pb::turn_finished::Outcome::Response(response)) => value(response),
                    Some(pb::turn_finished::Outcome::Error(error)) => {
                        Err(error.decode().map_err(protocol)?)
                    }
                    None => Err(crate::acp::Error::internal_error()),
                };
            }
            Some(pb::event::Kind::TurnInterrupted(interrupted)) if interrupted.turn == turn => {
                return Err(protocol(interrupted.message));
            }
            _ => {}
        }
    }
    Err(protocol("session ended before the prompt result"))
}

async fn callbacks<B: Bas + 'static>(
    backend: Arc<B>,
    connection: Arc<Connection>,
    session: String,
    cx: ConnectionTo<Client>,
) -> Result<(), crate::acp::Error> {
    let permissions = backend
        .watch_permissions(Request::new(pb::WatchPermissionsRequest {
            session: session.clone(),
        }))
        .await
        .map_err(status)?
        .into_inner();
    let mut permissions = Box::pin(permissions);
    let permissions_backend = backend.clone();
    let permission_session = session.clone();
    let peer = cx.clone();
    cx.spawn(async move {
        let mut pending = std::collections::HashMap::new();
        while let Some(snapshot) = permissions.next().await {
            let snapshot = snapshot.map_err(status)?;
            let ids: std::collections::HashSet<_> = snapshot
                .pending
                .iter()
                .map(|entry| entry.request_id.clone())
                .collect();
            pending.retain(
                |id, cancellation: &mut agent_client_protocol::RequestCancellationHandle| {
                    if ids.contains(id) {
                        true
                    } else {
                        let _ = cancellation.cancel();
                        false
                    }
                },
            );
            for permission in snapshot.pending {
                if pending.contains_key(&permission.request_id) {
                    continue;
                }
                let request = peer.prepare_request(UntypedMessage::new(
                    "session/request_permission",
                    value(
                        permission
                            .request
                            .ok_or_else(crate::acp::Error::invalid_params)?,
                    )?,
                )?);
                pending.insert(permission.request_id.clone(), request.cancellation_handle());
                let backend = permissions_backend.clone();
                let session = permission_session.clone();
                request.on_receiving_result(async move |result| {
                    let Ok(response) = result else { return Ok(()) };
                    let response = backend
                        .respond_permission(Request::new(pb::RespondPermissionRequest {
                            session,
                            request_id: permission.request_id,
                            response: Some(payload(response)),
                        }))
                        .await;
                    // Cancellation can remove the backend request before its response arrives.
                    if let Err(error) = response
                        && error.code() != tonic::Code::FailedPrecondition
                    {
                        return Err(status(error));
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    })?;
    let elicitations = backend
        .watch_elicitations(Request::new(pb::WatchElicitationsRequest {
            session: session.clone(),
        }))
        .await
        .map_err(status)?
        .into_inner();
    let mut elicitations = Box::pin(elicitations);
    let peer = cx.clone();
    cx.spawn(async move {
        let mut pending = std::collections::HashMap::new();
        let mut completed = std::collections::HashSet::new();
        while let Some(snapshot) = elicitations.next().await {
            let snapshot = snapshot.map_err(status)?;
            let ids: std::collections::HashSet<_> = snapshot
                .pending
                .iter()
                .filter(|entry| !entry.answered)
                .map(|entry| entry.request_id.clone())
                .collect();
            pending.retain(
                |id, cancellation: &mut agent_client_protocol::RequestCancellationHandle| {
                    if ids.contains(id) {
                        true
                    } else {
                        let _ = cancellation.cancel();
                        false
                    }
                },
            );
            for id in snapshot.completed_ids {
                if completed.insert(id.clone()) {
                    peer.send_notification(UntypedMessage::new(
                        "elicitation/complete",
                        json!({"elicitationId": id}),
                    )?)?;
                }
            }
            for elicitation in snapshot.pending {
                if elicitation.answered || pending.contains_key(&elicitation.request_id) {
                    continue;
                }
                let mut params = value(
                    elicitation
                        .request
                        .ok_or_else(crate::acp::Error::invalid_params)?,
                )?;
                // Backend request IDs belong to another connection. This facade owns the session scope.
                let fields = params
                    .as_object_mut()
                    .ok_or_else(crate::acp::Error::invalid_params)?;
                fields.remove("requestId");
                fields.insert("sessionId".into(), json!(session));
                if !connection.supports_elicitation(&params) {
                    continue;
                }
                let request =
                    peer.prepare_request(UntypedMessage::new("elicitation/create", params)?);
                pending.insert(
                    elicitation.request_id.clone(),
                    request.cancellation_handle(),
                );
                let backend = backend.clone();
                let session = session.clone();
                request.on_receiving_result(async move |result| {
                    let Ok(answer) = result else { return Ok(()) };
                    let response = backend
                        .respond_elicitation(Request::new(pb::RespondElicitationRequest {
                            session,
                            request_id: elicitation.request_id,
                            response: Some(payload(answer)),
                        }))
                        .await;
                    // Cancellation can remove the backend request before its response arrives.
                    if let Err(error) = response
                        && error.code() != tonic::Code::FailedPrecondition
                    {
                        return Err(status(error));
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    })?;
    Ok(())
}
