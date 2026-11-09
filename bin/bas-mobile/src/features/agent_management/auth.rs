use crate::{
    entities::agent::{Key, LoginStatus},
    shared::{api, i18n::tr, routes},
};
use bas_client::{
    authentication::{self, Execution, Method, elicitation},
    management::{Catalog, Entry},
    service::pb,
};
use nsh_core::prelude::*;
use std::time::Duration;

#[derive(Clone, Default)]
pub(crate) struct State {
    pub key: Key,
    pub entry: Option<Entry>,
    pub methods: Vec<Method>,
    pub message: String,
    pub error: String,
    flow: Flow,
    epoch: u64,
}

#[derive(Clone, Default)]
enum Flow {
    #[default]
    Idle,
    Discovering,
    Ready,
    Authenticating {
        tx: tokio::sync::mpsc::Sender<pb::AuthenticationInput>,
        pending: Vec<pb::PendingElicitation>,
        verification: Verification,
    },
}

#[derive(Clone)]
enum Verification {
    Idle,
    Checking,
}

impl State {
    pub fn loading(&self) -> bool {
        matches!(self.flow, Flow::Discovering)
    }
    pub fn running(&self) -> bool {
        matches!(self.flow, Flow::Authenticating { .. })
    }
    pub fn checking(&self) -> bool {
        matches!(
            self.flow,
            Flow::Authenticating {
                verification: Verification::Checking,
                ..
            }
        )
    }
    pub fn pending(&self) -> &[pb::PendingElicitation] {
        match &self.flow {
            Flow::Authenticating { pending, .. } => pending,
            _ => &[],
        }
    }
    fn pending_mut(&mut self) -> Option<&mut Vec<pb::PendingElicitation>> {
        match &mut self.flow {
            Flow::Authenticating { pending, .. } => Some(pending),
            _ => None,
        }
    }
    fn sender(&self) -> Option<tokio::sync::mpsc::Sender<pb::AuthenticationInput>> {
        match &self.flow {
            Flow::Authenticating { tx, .. } => Some(tx.clone()),
            _ => None,
        }
    }
    fn verify(&mut self, next: Verification) {
        if let Flow::Authenticating { verification, .. } = &mut self.flow {
            *verification = next;
        }
    }
}
pub(crate) fn state() -> Signal<State> {
    global::<State>()
}

pub(crate) fn open(entry: Entry) {
    open_inner(entry.id.clone(), Some(entry));
}
pub(crate) fn open_agent(agent: String) {
    open_inner(agent, None);
}

fn open_inner(agent: String, entry: Option<Entry>) {
    let epoch = state().get_untracked().epoch + 1;
    state().set(State {
        key: Key::new(
            api::context()
                .map(|context| context.backend)
                .unwrap_or_default(),
            agent,
        ),
        entry,
        epoch,
        ..Default::default()
    });
    routes::authentication();
    reload();
}

pub(crate) fn reload() {
    cancel();
    let Some(context) = api::context() else {
        state().update(|s| s.error = tr!("offline"));
        return;
    };
    state().update(|s| {
        s.flow = Flow::Discovering;
        s.error.clear();
        s.message.clear();
    });
    let snapshot = state().get_untracked();
    api::spawn_task(async move {
        let mut client = context.client.clone();
        let result = background(async move {
            let entry = match snapshot.entry {
                Some(entry) => entry,
                None => {
                    let catalog = client
                        .list_agent_catalog(pb::ListAgentCatalogRequest { refresh: false })
                        .await?
                        .into_inner();
                    serde_json::from_slice::<Catalog>(&catalog.catalog_json)?
                        .entries
                        .into_iter()
                        .find(|entry| entry.id == snapshot.key.agent)
                        .ok_or_else(|| anyhow::anyhow!(tr!("auth-agent-unavailable")))?
                }
            };
            let spec = entry
                .spec
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(tr!("auth-agent-unavailable")))?;
            let check = client
                .check_agent(pb::CheckAgentRequest {
                    agent: entry.id.clone(),
                    expected_spec_json: serde_json::to_vec(spec)?,
                    connect: true,
                })
                .await?
                .into_inner();
            anyhow::Ok((entry, check))
        })
        .await;
        if !context.current() || state().get_untracked().epoch != snapshot.epoch {
            return;
        }
        match result {
            Ok(Ok((entry, check))) => {
                state().update(|s| s.entry = Some(entry));
                apply_check(check);
            }
            Ok(Err(error)) => finish(error.to_string()),
            Err(error) => finish(error.to_string()),
        }
    });
}

pub(crate) fn recheck() {
    let snapshot = state().get_untracked();
    if snapshot.loading() || snapshot.checking() {
        return;
    }
    if !snapshot.running() {
        reload();
        return;
    }
    let Some(spec) = snapshot
        .entry
        .as_ref()
        .and_then(|entry| entry.spec.as_ref())
    else {
        return;
    };
    let Some(context) = api::context() else {
        return;
    };
    let request = pb::CheckAgentRequest {
        agent: snapshot.key.agent,
        expected_spec_json: serde_json::to_vec(spec).expect("agent spec serializes"),
        connect: true,
    };
    state().update(|s| {
        s.verify(Verification::Checking);
        s.error.clear();
    });
    api::spawn_task(async move {
        let mut client = context.client.clone();
        let result = background(async move { client.check_agent(request).await }).await;
        let current = state().get_untracked();
        if !context.current() || current.epoch != snapshot.epoch || !current.running() {
            return;
        }
        state().update(|s| s.verify(Verification::Idle));
        match result {
            Ok(Ok(response)) => {
                let check = response.into_inner();
                match check.status() {
                    pb::AgentCheckStatus::Connected => {
                        // Persisted credentials can be ready while authenticate is
                        // still waiting. Retire that stream before publishing success.
                        complete(check);
                    }
                    pb::AgentCheckStatus::AuthenticationRequired => {}
                    _ => state().update(|s| s.error = check.message),
                }
            }
            Ok(Err(error)) => state().update(|s| s.error = error.message().to_owned()),
            Err(error) => state().update(|s| s.error = error.to_string()),
        }
    });
}

fn apply_check(check: pb::CheckAgentResponse) {
    let methods = (|| {
        let initialized = check
            .initialization
            .as_ref()
            .map(|p| p.decode())
            .transpose()?
            .unwrap_or(serde_json::json!({}));
        let error = check
            .authentication_error
            .as_ref()
            .map(|p| p.decode())
            .transpose()?;
        Ok::<_, anyhow::Error>(authentication::methods(&initialized, error.as_ref())?)
    })();
    batch(|| {
        state().get_untracked().key.checked(&check);
        state().update(|s| {
            s.flow = Flow::Ready;
            match methods {
                Ok(methods) => s.methods = methods,
                Err(error) => s.error = error.to_string(),
            }
            s.message = match check.status() {
                pb::AgentCheckStatus::Connected => tr!("auth-complete"),
                pb::AgentCheckStatus::AuthenticationRequired => tr!("auth-choose-method"),
                _ => check.message.clone(),
            };
        });
    });
}

fn complete(check: pb::CheckAgentResponse) {
    batch(|| {
        cancel();
        apply_check(check);
    });
}

pub(crate) fn connected() -> bool {
    state().with(|s| matches!(s.flow, Flow::Ready) && s.key.status() == LoginStatus::Ready)
}
pub(crate) fn cancel() {
    state().update(|s| {
        s.epoch += 1;
        s.flow = Flow::Idle;
    });
}
pub(crate) fn stop() {
    cancel();
    state().update(|s| s.message = tr!("auth-cancelled"));
}

pub(crate) fn start(method: Method) {
    let snapshot = state().get_untracked();
    if snapshot.running() || snapshot.loading() || !snapshot.methods.contains(&method) {
        return;
    }
    match method.execution {
        Execution::Agent => {}
        Execution::Terminal(_) => {
            state().update(|s| s.error = tr!("terminal-auth-required"));
            return;
        }
        Execution::Unsupported { reason } => {
            state().update(|s| s.error = reason);
            return;
        }
    }
    let Some(entry) = snapshot.entry else { return };
    let Some(spec) = entry.spec else { return };
    let Some(context) = api::context() else {
        return;
    };
    cancel();
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let start = pb::AuthenticateAgentRequest {
        agent: entry.id,
        expected_spec_json: serde_json::to_vec(&spec).expect("agent spec serializes"),
        method_id: method.id.clone(),
        expected_method_json: serde_json::to_vec(&method).expect("auth method serializes"),
    };
    tx.try_send(pb::AuthenticationInput {
        kind: Some(pb::authentication_input::Kind::Start(start)),
    })
    .expect("new authentication channel has capacity");
    state().update(|s| {
        s.flow = Flow::Authenticating {
            tx,
            pending: Vec::new(),
            verification: Verification::Idle,
        };
        s.error.clear();
        s.message = tr!("awaiting-auth");
    });
    let epoch = state().get_untracked().epoch;
    api::spawn_task(async move {
        let mut client = context.client.clone();
        let result = background(async move {
            client
                .authenticate_agent_interactive(tokio_stream::wrappers::ReceiverStream::new(rx))
                .await
        })
        .await;
        if !context.current() || state().get_untracked().epoch != epoch {
            return;
        }
        let mut stream = match result {
            Ok(Ok(response)) => response.into_inner(),
            Ok(Err(error)) => {
                finish(error.message().to_owned());
                return;
            }
            Err(error) => {
                finish(error.to_string());
                return;
            }
        };
        loop {
            let result = background(async move {
                let result = tokio::time::timeout(Duration::from_secs(1), stream.message()).await;
                (stream, result)
            })
            .await;
            if !context.current() || state().get_untracked().epoch != epoch {
                return;
            }
            let event = match result {
                Ok((next, Ok(Ok(Some(event))))) => {
                    stream = next;
                    event
                }
                Ok((next, Err(_))) => {
                    stream = next;
                    continue;
                }
                Ok((_, Ok(Ok(None)))) => {
                    finish(tr!("auth-stream-ended"));
                    return;
                }
                Ok((_, Ok(Err(error)))) => {
                    finish(error.message().to_owned());
                    return;
                }
                Err(error) => {
                    finish(error.to_string());
                    return;
                }
            };
            match event.kind {
                Some(pb::authentication_event::Kind::Check(check)) => {
                    // Publish completion after the final check, not after a form response.
                    complete(check);
                    return;
                }
                Some(pb::authentication_event::Kind::Elicitation(pending)) => state().update(|s| {
                    if let Some(requests) = s.pending_mut() {
                        requests.retain(|p| p.request_id != pending.request_id);
                        requests.push(pending);
                    }
                }),
                Some(pb::authentication_event::Kind::ElicitationCancelled(id)) => {
                    state().update(|s| {
                        if let Some(pending) = s.pending_mut() {
                            pending.retain(|p| p.request_id != id);
                        }
                    });
                }
                Some(pb::authentication_event::Kind::ElicitationComplete(payload)) => {
                    if let Ok(value) = payload.decode::<serde_json::Value>() {
                        state().update(|s| { if let Some(pending) = s.pending_mut() { pending.retain(|p| {
                            !request(p).is_ok_and(|r| matches!(r.mode, elicitation::Mode::Url(url) if value.get("elicitationId").and_then(|v| v.as_str()) == Some(url.elicitation_id.as_str())))
                        }); } });
                    }
                }
                Some(pb::authentication_event::Kind::Terminal(_)) => {
                    finish(tr!("terminal-auth-required"));
                    return;
                }
                Some(pb::authentication_event::Kind::Progress(message)) => {
                    state().update(|s| s.message = message)
                }
                None => {}
            }
        }
    });
}

fn finish(error: String) {
    state().update(|s| {
        s.flow = Flow::Idle;
        s.error = error;
        s.message.clear();
    });
}

/// Decode an agent's form or browser authorization request.
/// # Errors
/// Returns missing, malformed or unsupported request errors for display.
pub(crate) fn request(pending: &pb::PendingElicitation) -> anyhow::Result<elicitation::Request> {
    let payload = pending
        .request
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(tr!("auth-invalid-request")))?;
    Ok(elicitation::parse_request(
        &payload.decode::<serde_json::Value>()?,
    )?)
}

pub(crate) fn answer(pending: pb::PendingElicitation, value: serde_json::Value) {
    let snapshot = state().get_untracked();
    if !snapshot
        .pending()
        .iter()
        .any(|p| p.request_id == pending.request_id && !p.answered)
    {
        return;
    }
    let Some(tx) = snapshot.sender() else { return };
    let retain = value.get("action").and_then(|v| v.as_str()) == Some("accept")
        && request(&pending).is_ok_and(|r| matches!(r.mode, elicitation::Mode::Url(_)));
    api::spawn_task(async move {
        let request_id = pending.request_id;
        let result = tx
            .send(pb::AuthenticationInput {
                kind: Some(pb::authentication_input::Kind::ElicitationResponse(
                    pb::ElicitationAnswer {
                        request_id: request_id.clone(),
                        response: Some(pb::AcpPayload {
                            json: serde_json::to_vec(&value).expect("response serializes"),
                        }),
                    },
                )),
            })
            .await;
        if state().get_untracked().epoch != snapshot.epoch {
            return;
        }
        match result {
            Ok(()) => state().update(|s| {
                let Some(requests) = s.pending_mut() else {
                    return;
                };
                if retain {
                    if let Some(p) = requests.iter_mut().find(|p| p.request_id == request_id) {
                        p.answered = true;
                    }
                } else {
                    requests.retain(|p| p.request_id != request_id);
                }
            }),
            Err(_) => finish(tr!("auth-stream-ended")),
        }
    });
}
