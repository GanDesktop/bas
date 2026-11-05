//! Optional server-side adapter over the Bas management/journal API.
//! Session execution remains owned by the backend, never by a transport task.
use crate::service::pb::{
    self,
    acp_transport_server::{AcpTransport, AcpTransportServer},
    bas_server::Bas,
};
use futures::{Stream, StreamExt};
use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc};
use tonic::{Request, Response, Status};

mod authentication;
mod connection;
mod session;

pub type AuthenticationInput =
    Pin<Box<dyn Stream<Item = Result<pb::AuthenticationInput, Status>> + Send>>;
pub type AuthenticationEvents =
    Pin<Box<dyn Stream<Item = Result<pb::AuthenticationEvent, Status>> + Send>>;

/// Daemon-owned authentication, shared by Bas RPC and ACP transports.
pub trait Authentication: Send + Sync + 'static {
    fn start(
        &self,
        input: AuthenticationInput,
    ) -> futures::future::BoxFuture<'static, Result<AuthenticationEvents, Status>>;
}

const PREPARE_TTL: Duration = Duration::from_secs(60);
const MAX_CONNECTIONS: usize = 1024;
const MAX_BINDINGS: usize = 1024;

struct Prepared {
    at: Instant,
    target: pb::prepare_acp_request::Target,
    interactive: bool,
}
#[derive(Default)]
struct State {
    prepared: HashMap<String, Prepared>,
    active: HashMap<String, Arc<Connection>>,
}
struct Connection {
    target: pb::prepare_acp_request::Target,
    interactive: bool,
    bindings: Mutex<HashMap<String, Binding>>,
    submitted: tokio::sync::Notify,
    capabilities: Mutex<serde_json::Value>,
    auxiliary: broadcast::Sender<pb::AcpAuxiliaryEvent>,
    authentication: Mutex<Option<mpsc::Sender<pb::AuthenticationInput>>>,
}
impl Connection {
    fn supports_elicitation(&self, params: &serde_json::Value) -> bool {
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
}

struct Binding {
    turn: String,
    outcome: Option<Result<pb::PromptSubmissionResponse, Status>>,
}

pub struct Gateway<B> {
    backend: Arc<B>,
    state: Arc<Mutex<State>>,
    authentication: Option<Arc<dyn Authentication>>,
}
impl<B> Clone for Gateway<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            state: self.state.clone(),
            authentication: self.authentication.clone(),
        }
    }
}
impl<B: Bas + 'static> Gateway<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
            state: Arc::default(),
            authentication: None,
        }
    }
    pub fn with_authentication(mut self, handler: impl Authentication) -> Self {
        self.authentication = Some(Arc::new(handler));
        self
    }
    pub fn into_server(self) -> AcpTransportServer<Self> {
        AcpTransportServer::new(self)
            .max_decoding_message_size(crate::service::MAX_MESSAGE_SIZE)
            .max_encoding_message_size(crate::service::MAX_MESSAGE_SIZE)
    }
    fn active(&self, id: &str) -> Result<Arc<Connection>, Status> {
        self.state
            .lock()
            .expect("connection state")
            .active
            .get(id)
            .cloned()
            .ok_or_else(|| Status::not_found("ACP connection is not active"))
    }
}

struct Lease {
    id: String,
    state: Arc<Mutex<State>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.state.lock().expect("connection state");
        state.active.remove(&self.id);
    }
}

#[tonic::async_trait]
impl<B: Bas + 'static> AcpTransport for Gateway<B> {
    async fn prepare(
        &self,
        request: Request<pb::PrepareAcpRequest>,
    ) -> Result<Response<pb::PreparedAcpConnection>, Status> {
        let request = request.into_inner();
        let target = request
            .target
            .ok_or_else(|| Status::invalid_argument("missing ACP target"))?;
        match &target {
            pb::prepare_acp_request::Target::Session(session) => {
                self.backend
                    .get_session(Request::new(pb::GetSessionRequest {
                        session: session.clone(),
                    }))
                    .await?;
            }
            pb::prepare_acp_request::Target::Authentication(auth) => {
                self.backend
                    .check_agent(Request::new(pb::CheckAgentRequest {
                        agent: auth.agent.clone(),
                        expected_spec_json: auth.expected_spec_json.clone(),
                        connect: false,
                    }))
                    .await?;
                if self.authentication.is_none() {
                    return Err(Status::unimplemented(
                        "interactive authentication unavailable",
                    ));
                }
            }
        }
        let mut state = self.state.lock().expect("connection state");
        state
            .prepared
            .retain(|_, entry| entry.at.elapsed() < PREPARE_TTL);
        if state.prepared.len() + state.active.len() >= MAX_CONNECTIONS {
            return Err(Status::resource_exhausted("too many ACP connections"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        state.prepared.insert(
            id.clone(),
            Prepared {
                at: Instant::now(),
                target,
                interactive: request.interactive,
            },
        );
        Ok(Response::new(pb::PreparedAcpConnection { id }))
    }

    type OpenStream = Pin<Box<dyn Stream<Item = Result<pb::AcpMessage, Status>> + Send>>;
    async fn open(
        &self,
        request: Request<tonic::Streaming<pb::AcpMessage>>,
    ) -> Result<Response<Self::OpenStream>, Status> {
        let id = request
            .metadata()
            .get(super::transport::CONNECTION_HEADER)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("missing prepared connection"))?
            .to_owned();
        let connection = {
            let mut state = self.state.lock().expect("connection state");
            let prepared = state
                .prepared
                .remove(&id)
                .filter(|entry| entry.at.elapsed() < PREPARE_TTL)
                .ok_or_else(|| {
                    Status::not_found("prepared connection expired or already consumed")
                })?;
            let connection = Arc::new(Connection {
                target: prepared.target,
                interactive: prepared.interactive,
                bindings: Mutex::default(),
                submitted: tokio::sync::Notify::new(),
                capabilities: Mutex::new(serde_json::Value::Null),
                auxiliary: broadcast::channel(32).0,
                authentication: Mutex::default(),
            });
            state.active.insert(id.clone(), connection.clone());
            connection
        };
        let lease = Lease {
            id,
            state: self.state.clone(),
        };
        let (send, receive) = mpsc::channel(32);
        let gateway = self.clone();
        let task = tokio::spawn(async move {
            let _lease = lease;
            let result =
                connection::serve(gateway, connection, request.into_inner(), send.clone()).await;
            if let Err(error) = result {
                let _ = send.send(Err(Status::unavailable(error.to_string()))).await;
            }
        });
        let stream =
            futures::stream::unfold((receive, Abort(task)), |(mut receive, task)| async move {
                receive
                    .recv()
                    .await
                    .map(|message| (message, (receive, task)))
            });
        Ok(Response::new(Box::pin(stream)))
    }
    async fn bind_submission(
        &self,
        request: Request<pb::BindAcpSubmissionRequest>,
    ) -> Result<Response<pb::AcpControlResponse>, Status> {
        let request = request.into_inner();
        let _: crate::model::TurnId = request
            .turn
            .parse()
            .map_err(|_| Status::invalid_argument("invalid turn"))?;
        let id: serde_json::Value = serde_json::from_str(&request.request_id)
            .map_err(|_| Status::invalid_argument("invalid request ID"))?;
        if !id.is_string() && !id.is_number() {
            return Err(Status::invalid_argument("invalid request ID"));
        }
        let connection = self.active(&request.connection)?;
        if !matches!(
            connection.target,
            pb::prepare_acp_request::Target::Session(_)
        ) {
            return Err(Status::failed_precondition("not a session connection"));
        }
        let mut bindings = connection.bindings.lock().expect("submission bindings");
        if bindings.contains_key(&request.request_id) {
            return Err(Status::already_exists("request already bound"));
        }
        if bindings.len() >= MAX_BINDINGS {
            return Err(Status::resource_exhausted("too many submissions"));
        }
        bindings.insert(
            request.request_id,
            Binding {
                turn: request.turn,
                outcome: None,
            },
        );
        Ok(Response::new(pb::AcpControlResponse {}))
    }
    async fn submission_state(
        &self,
        request: Request<pb::BindAcpSubmissionRequest>,
    ) -> Result<Response<pb::PromptSubmissionResponse>, Status> {
        let request = request.into_inner();
        let connection = self.active(&request.connection)?;
        loop {
            let changed = connection.submitted.notified();
            {
                let mut bindings = connection.bindings.lock().expect("submission bindings");
                let binding = bindings
                    .get(&request.request_id)
                    .filter(|binding| binding.turn == request.turn)
                    .ok_or_else(|| Status::not_found("unknown submission binding"))?;
                if let Some(outcome) = binding.outcome.clone() {
                    bindings.remove(&request.request_id);
                    return outcome.map(Response::new);
                }
            }
            changed.await;
        }
    }
    type WatchAuxiliaryStream =
        Pin<Box<dyn Stream<Item = Result<pb::AcpAuxiliaryEvent, Status>> + Send>>;
    async fn watch_auxiliary(
        &self,
        request: Request<pb::AcpConnectionRequest>,
    ) -> Result<Response<Self::WatchAuxiliaryStream>, Status> {
        let connection = self.active(&request.into_inner().connection)?;
        let receive = connection.auxiliary.subscribe();
        let stream = futures::stream::unfold(receive, |mut receive| async move {
            match receive.recv().await {
                Ok(event) => Some((Ok(event), receive)),
                Err(broadcast::error::RecvError::Closed) => None,
                Err(broadcast::error::RecvError::Lagged(_)) => Some((
                    Err(Status::data_loss("authentication control stream lagged")),
                    receive,
                )),
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }
    async fn terminal_result(
        &self,
        request: Request<pb::AcpTerminalResultRequest>,
    ) -> Result<Response<pb::AcpControlResponse>, Status> {
        let request = request.into_inner();
        let connection = self.active(&request.connection)?;
        let input = connection
            .authentication
            .lock()
            .expect("authentication input")
            .clone()
            .ok_or_else(|| Status::failed_precondition("no authentication in progress"))?;
        input
            .send(pb::AuthenticationInput {
                kind: Some(pb::authentication_input::Kind::TerminalResult(
                    pb::TerminalAuthenticationResult {
                        exit_code: request.exit_code,
                    },
                )),
            })
            .await
            .map_err(|_| Status::cancelled("authentication closed"))?;
        Ok(Response::new(pb::AcpControlResponse {}))
    }
}
struct Abort(tokio::task::JoinHandle<()>);
impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}
