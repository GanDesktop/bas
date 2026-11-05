use super::*;
use agent_client_protocol::{
    Agent, Client, ConnectionTo, Lines, UntypedMessage, on_receive_notification, on_receive_request,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU8, Ordering};

pub(super) fn protocol(error: impl std::fmt::Display) -> crate::acp::Error {
    crate::acp::Error::internal_error().data(error.to_string())
}
pub(super) fn status(error: Status) -> crate::acp::Error {
    bas_protocol::protocol::acp_error(&error).unwrap_or_else(|_| protocol(error.message()))
}
pub(super) fn payload(value: Value) -> pb::AcpPayload {
    pb::AcpPayload {
        json: serde_json::to_vec(&value).expect("protocol JSON"),
    }
}
pub(super) fn value(payload: pb::AcpPayload) -> Result<Value, crate::acp::Error> {
    serde_json::from_slice(&payload.json).map_err(protocol)
}
pub(super) async fn serve<B: Bas + 'static>(
    gateway: Gateway<B>,
    connection: Arc<Connection>,
    incoming: tonic::Streaming<pb::AcpMessage>,
    send: mpsc::Sender<Result<pb::AcpMessage, Status>>,
) -> Result<(), crate::acp::Error> {
    let outgoing = futures::sink::unfold(send, |send, line: String| async move {
        send.send(Ok(pb::AcpMessage {
            json: line.into_bytes(),
        }))
        .await
        .map_err(std::io::Error::other)?;
        Ok::<_, std::io::Error>(send)
    });
    let incoming = incoming.map(|frame| {
        String::from_utf8(frame.map_err(std::io::Error::other)?.json).map_err(std::io::Error::other)
    });
    let initialized = Arc::new(AtomicU8::new(0));
    let loaded = Arc::new(AtomicU8::new(0));
    let cancel_loaded = loaded.clone();
    let cancel_backend = gateway.backend.clone();
    let cancel_connection = connection.clone();
    Agent
        .builder()
        .on_receive_request(
            async move |request: UntypedMessage, responder, cx: ConnectionTo<Client>| {
                let gateway = gateway.clone();
                let connection = connection.clone();
                let initialized = initialized.clone();
                let loaded = loaded.clone();
                cx.clone().spawn(async move {
                    let id = serde_json::to_string(responder.id()).map_err(protocol)?;
                    let result =
                        dispatch(&gateway, connection, initialized, loaded, request, id, cx).await;
                    responder.respond_with_result(result)
                })?;
                Ok(())
            },
            on_receive_request!(),
        )
        .on_receive_notification(
            async move |notification: UntypedMessage, cx| {
                if notification.method == "session/cancel"
                    && cancel_loaded.load(Ordering::Acquire) == 2
                {
                    let backend = cancel_backend.clone();
                    let target = cancel_connection.clone();
                    cx.spawn(async move {
                        session::validate(&target, &notification.params)?;
                        backend
                            .cancel(Request::new(payload(notification.params)))
                            .await
                            .map_err(status)?;
                        Ok(())
                    })?;
                }
                Ok(())
            },
            on_receive_notification!(),
        )
        .connect_to(Lines::new(outgoing, incoming))
        .await
}

async fn dispatch<B: Bas + 'static>(
    gateway: &Gateway<B>,
    connection: Arc<Connection>,
    initialized: Arc<AtomicU8>,
    loaded: Arc<AtomicU8>,
    request: UntypedMessage,
    id: String,
    cx: ConnectionTo<Client>,
) -> Result<Value, crate::acp::Error> {
    if request.method == "initialize" {
        let init: crate::acp::InitializeRequest = serde_json::from_value(request.params)
            .map_err(|_| crate::acp::Error::invalid_params())?;
        if initialized
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(crate::acp::Error::invalid_request());
        }
        if init.protocol_version != agent_client_protocol::schema::ProtocolVersion::V1 {
            return Err(crate::acp::Error::invalid_params());
        }
        let result = match &connection.target {
            pb::prepare_acp_request::Target::Session(session) => {
                let snapshot = gateway
                    .backend
                    .get_session(Request::new(pb::GetSessionRequest {
                        session: session.clone(),
                    }))
                    .await
                    .map_err(status)?
                    .into_inner()
                    .session
                    .ok_or_else(crate::acp::Error::internal_error)?;
                let mut result = snapshot
                    .initialization
                    .map(value)
                    .transpose()?
                    .unwrap_or(json!({"protocolVersion":1,"agentCapabilities":{}}));
                // The facade implements load/replay itself and does not offer fresh sessions.
                result["protocolVersion"] = json!(1);
                result["agentInfo"] = json!({"name":"bas","version":env!("CARGO_PKG_VERSION")});
                let prompt = result["agentCapabilities"]
                    .get("promptCapabilities")
                    .cloned();
                result["agentCapabilities"] = json!({"loadSession":true});
                if let Some(prompt) = prompt {
                    result["agentCapabilities"]["promptCapabilities"] = prompt;
                }
                result["authMethods"] = json!([]);
                result
            }
            pb::prepare_acp_request::Target::Authentication(auth) => {
                let check = gateway
                    .backend
                    .check_agent(Request::new(pb::CheckAgentRequest {
                        agent: auth.agent.clone(),
                        expected_spec_json: auth.expected_spec_json.clone(),
                        connect: true,
                    }))
                    .await
                    .map_err(status)?
                    .into_inner();
                let mut result = value(
                    check
                        .initialization
                        .ok_or_else(crate::acp::Error::auth_required)?,
                )?;
                result["agentCapabilities"] = json!({});
                result["agentInfo"] =
                    json!({"name":"bas-auth","version":env!("CARGO_PKG_VERSION")});
                result
            }
        };
        *connection
            .capabilities
            .lock()
            .expect("ACP client capabilities") =
            serde_json::to_value(init.client_capabilities).map_err(protocol)?;
        initialized.store(2, Ordering::Release);
        return Ok(result);
    }
    if initialized.load(Ordering::Acquire) != 2 {
        return Err(crate::acp::Error::invalid_request());
    }
    match &connection.target {
        pb::prepare_acp_request::Target::Session(_) => {
            if request.method == "session/load" {
                if loaded
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    return Err(crate::acp::Error::invalid_request());
                }
                let result =
                    session::load(gateway.backend.clone(), connection, request.params, cx).await;
                loaded.store(if result.is_ok() { 2 } else { 0 }, Ordering::Release);
                return result;
            }
            if loaded.load(Ordering::Acquire) != 2 {
                return Err(crate::acp::Error::invalid_request());
            }
            session::request(gateway.backend.clone(), connection, request).await
        }
        pb::prepare_acp_request::Target::Authentication(_) if request.method == "authenticate" => {
            authentication::run(gateway, connection, request.params, id, cx).await
        }
        _ => Err(crate::acp::Error::method_not_found()),
    }
}
