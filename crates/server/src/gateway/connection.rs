use super::protocol::{payload, protocol, status, value};
use super::*;
use agent_client_protocol::{
    Agent, Client, ConnectionTo, Lines, UntypedMessage, on_receive_notification, on_receive_request,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU8, Ordering};

/// One prepared ACP connection and its handshake state.
pub(super) struct Connection {
    pub(super) target: pb::prepare_acp_request::Target,
    pub(super) interactive: bool,
    pub(super) callbacks_only: bool,
    capabilities: Mutex<Value>,
    pub(super) auxiliary: broadcast::Sender<pb::AcpAuxiliaryEvent>,
    pub(super) authentication: Mutex<Option<mpsc::Sender<pb::AuthenticationInput>>>,
    initialized: Handshake,
    loaded: Handshake,
}

#[repr(u8)]
enum Stage {
    Pending,
    InProgress,
    Ready,
}

#[derive(Default)]
struct Handshake(AtomicU8);

impl Handshake {
    /// # Errors
    /// Rejects repeated or concurrent initialization of the same handshake.
    fn begin(&self) -> Result<(), crate::acp::Error> {
        self.0
            .compare_exchange(
                Stage::Pending as u8,
                Stage::InProgress as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| crate::acp::Error::invalid_request())
    }

    fn finish(&self, succeeded: bool) {
        let stage = if succeeded {
            Stage::Ready
        } else {
            Stage::Pending
        };
        self.0.store(stage as u8, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.0.load(Ordering::Acquire) == Stage::Ready as u8
    }
}

impl Connection {
    pub(super) fn new(prepared: Prepared) -> Self {
        Self {
            target: prepared.target,
            interactive: prepared.interactive,
            callbacks_only: prepared.callbacks_only,
            capabilities: Mutex::new(Value::Null),
            auxiliary: broadcast::channel(32).0,
            authentication: Mutex::default(),
            initialized: Handshake::default(),
            loaded: Handshake::default(),
        }
    }

    /// # Errors
    /// Returns an error if the request does not target this connection’s session.
    pub(super) fn validate_session(&self, params: &Value) -> Result<String, crate::acp::Error> {
        let pb::prepare_acp_request::Target::Session(session) = &self.target else {
            return Err(crate::acp::Error::invalid_params());
        };
        if params.get("sessionId").and_then(Value::as_str) != Some(session.as_str()) {
            return Err(crate::acp::Error::invalid_params());
        }
        Ok(session.clone())
    }

    pub(super) fn supports_elicitation(&self, params: &serde_json::Value) -> bool {
        let capabilities = self.capabilities.lock().expect("ACP client capabilities");
        let mode = params
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("form");
        matches!(mode, "form" | "url")
            && capabilities
                .get("elicitation")
                .and_then(|value| value.get(mode))
                .is_some_and(serde_json::Value::is_object)
    }

    /// # Errors
    /// Returns an error if the ACP connection fails.
    pub(super) async fn serve<B: Bas + 'static>(
        self: Arc<Self>,
        gateway: Gateway<B>,
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
            String::from_utf8(frame.map_err(std::io::Error::other)?.json)
                .map_err(std::io::Error::other)
        });
        let cancel_backend = gateway.backend.clone();
        let cancel_connection = self.clone();
        Agent
            .builder()
            .on_receive_request(
                async move |request: UntypedMessage, responder, cx: ConnectionTo<Client>| {
                    let gateway = gateway.clone();
                    let connection = self.clone();
                    cx.clone().spawn(async move {
                        let id = serde_json::to_string(responder.id()).map_err(protocol)?;
                        let result = connection.dispatch(&gateway, request, id, cx).await;
                        responder.respond_with_result(result)
                    })?;
                    Ok(())
                },
                on_receive_request!(),
            )
            .on_receive_notification(
                async move |notification: UntypedMessage, cx| {
                    if notification.method == "session/cancel"
                        && cancel_connection.loaded.is_ready()
                    {
                        let backend = cancel_backend.clone();
                        let target = cancel_connection.clone();
                        cx.spawn(async move {
                            target.validate_session(&notification.params)?;
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

    /// # Errors
    /// Returns an error if request validation or handling fails.
    async fn dispatch<B: Bas + 'static>(
        self: Arc<Self>,
        gateway: &Gateway<B>,
        request: UntypedMessage,
        id: String,
        cx: ConnectionTo<Client>,
    ) -> Result<Value, crate::acp::Error> {
        if request.method == "initialize" {
            let init: crate::acp::InitializeRequest = serde_json::from_value(request.params)
                .map_err(|_| crate::acp::Error::invalid_params())?;
            self.initialized.begin()?;
            if init.protocol_version != agent_client_protocol::schema::ProtocolVersion::V1 {
                return Err(crate::acp::Error::invalid_params());
            }
            let result = match &self.target {
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
            *self.capabilities.lock().expect("ACP client capabilities") =
                serde_json::to_value(init.client_capabilities).map_err(protocol)?;
            self.initialized.finish(true);
            return Ok(result);
        }
        if !self.initialized.is_ready() {
            return Err(crate::acp::Error::invalid_request());
        }
        match &self.target {
            pb::prepare_acp_request::Target::Session(_) => {
                if request.method == "session/load" {
                    self.loaded.begin()?;
                    let result = gateway.load_session(self.clone(), request.params, cx).await;
                    self.loaded.finish(result.is_ok());
                    return result;
                }
                if !self.loaded.is_ready() {
                    return Err(crate::acp::Error::invalid_request());
                }
                gateway.session_request(self, request).await
            }
            pb::prepare_acp_request::Target::Authentication(_)
                if request.method == "authenticate" =>
            {
                self.authenticate(gateway, request.params, id, cx).await
            }
            _ => Err(crate::acp::Error::method_not_found()),
        }
    }
}
