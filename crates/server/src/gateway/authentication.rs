use super::connection::{payload, protocol, status, value};
use super::*;
use agent_client_protocol::{Client, ConnectionTo, UntypedMessage};
use serde_json::{Value, json};
use tokio_stream::wrappers::ReceiverStream;

pub(super) async fn run<B: Bas + 'static>(
    gateway: &Gateway<B>,
    connection: Arc<Connection>,
    params: Value,
    request_id: String,
    peer: ConnectionTo<Client>,
) -> Result<Value, crate::acp::Error> {
    let request: crate::acp::AuthenticateRequest =
        serde_json::from_value(params).map_err(|_| crate::acp::Error::invalid_params())?;
    let pb::prepare_acp_request::Target::Authentication(prepared) = &connection.target else {
        return Err(crate::acp::Error::invalid_params());
    };
    if request.method_id.to_string() != prepared.method_id {
        return Err(crate::acp::Error::invalid_params());
    }
    let (send, receive) = mpsc::channel(16);
    {
        let mut active = connection
            .authentication
            .lock()
            .expect("authentication input");
        if active.is_some() {
            return Err(crate::acp::Error::invalid_request());
        }
        *active = Some(send.clone());
    }
    let _guard = AuthenticationGuard(connection.clone());
    send.send(pb::AuthenticationInput {
        kind: Some(pb::authentication_input::Kind::Start(prepared.clone())),
    })
    .await
    .map_err(protocol)?;
    let handler = gateway
        .authentication
        .as_ref()
        .ok_or_else(crate::acp::Error::method_not_found)?;
    let mut events = handler
        .start(Box::pin(ReceiverStream::new(receive).map(Ok)))
        .await
        .map_err(status)?;
    let mut pending = HashMap::new();
    while let Some(event) = events.next().await {
        use pb::authentication_event::Kind;
        match event.map_err(status)?.kind {
            Some(Kind::Check(check)) => {
                if check.status == pb::AgentCheckStatus::Connected as i32 {
                    return Ok(json!({}));
                }
                return Err(check
                    .authentication_error
                    .map(|error| error.decode().map_err(protocol))
                    .transpose()?
                    .unwrap_or_else(crate::acp::Error::auth_required));
            }
            Some(Kind::Elicitation(entry)) => {
                let mut params = value(
                    entry
                        .request
                        .ok_or_else(crate::acp::Error::invalid_params)?,
                )?;
                params["requestId"] = serde_json::from_str(&request_id).map_err(protocol)?;
                if !connection.supports_elicitation(&params) {
                    return Err(crate::acp::Error::invalid_request()
                        .data("client did not advertise this elicitation capability"));
                }
                let request =
                    peer.prepare_request(UntypedMessage::new("elicitation/create", params)?);
                pending.insert(entry.request_id.clone(), request.cancellation_handle());
                let send = send.clone();
                request.on_receiving_result(async move |result| {
                    send.send(pb::AuthenticationInput {
                        kind: Some(pb::authentication_input::Kind::ElicitationResponse(
                            pb::ElicitationAnswer {
                                request_id: entry.request_id,
                                response: Some(payload(
                                    result.unwrap_or(json!({"action":"cancel"})),
                                )),
                            },
                        )),
                    })
                    .await
                    .map_err(protocol)?;
                    Ok(())
                })?;
            }
            Some(Kind::ElicitationComplete(payload)) => {
                peer.send_notification(UntypedMessage::new(
                    "elicitation/complete",
                    value(payload)?,
                )?)?;
            }
            Some(Kind::ElicitationCancelled(id)) => {
                if let Some(request) = pending.remove(&id) {
                    let _ = request.cancel();
                }
            }
            Some(Kind::Terminal(terminal)) => {
                connection
                    .auxiliary
                    .send(pb::AcpAuxiliaryEvent {
                        kind: Some(pb::acp_auxiliary_event::Kind::Terminal(terminal)),
                    })
                    .map_err(|_| protocol("no terminal authentication handler attached"))?;
            }
            Some(Kind::Progress(progress)) => {
                let _ = connection.auxiliary.send(pb::AcpAuxiliaryEvent {
                    kind: Some(pb::acp_auxiliary_event::Kind::Progress(progress)),
                });
            }
            None => return Err(protocol("empty authentication event")),
        }
    }
    Err(protocol("authentication ended without a result"))
}
struct AuthenticationGuard(Arc<Connection>);
impl Drop for AuthenticationGuard {
    fn drop(&mut self) {
        self.0
            .authentication
            .lock()
            .expect("authentication input")
            .take();
    }
}
