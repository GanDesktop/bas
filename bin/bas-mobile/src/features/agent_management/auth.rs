use crate::shared::{api, i18n::tr, routes};
use bas_client::{
    authentication::{self, Execution, Method, elicitation},
    management::{Catalog, Entry},
    service::pb,
};
use nsh_core::prelude::*;
use std::time::Duration;

#[derive(Clone, Default)]
pub(crate) struct State {
    pub agent: String,
    pub entry: Option<Entry>,
    pub check: Option<pb::CheckAgentResponse>,
    pub methods: Vec<Method>,
    pub loading: bool,
    pub running: bool,
    pub pending: Vec<pb::PendingElicitation>,
    pub message: String,
    pub error: String,
    tx: Option<tokio::sync::mpsc::Sender<pb::AuthenticationInput>>,
    epoch: u64,
}
pub(crate) fn state() -> Signal<State> {
    global::<State>()
}

pub(crate) fn open(entry: Entry, check: Option<pb::CheckAgentResponse>) {
    open_inner(entry.id.clone(), Some(entry), check);
}
pub(crate) fn open_agent(agent: String) {
    open_inner(agent, None, None);
}

fn open_inner(agent: String, entry: Option<Entry>, check: Option<pb::CheckAgentResponse>) {
    let epoch = state().get_untracked().epoch + 1;
    state().set(State {
        agent,
        entry,
        epoch,
        ..Default::default()
    });
    routes::authentication();
    if let Some(check) = check {
        apply_check(check);
    } else {
        reload();
    }
}

pub(crate) fn reload() {
    cancel();
    let Some(context) = api::context() else {
        state().update(|s| s.error = tr!("offline"));
        return;
    };
    state().update(|s| {
        s.loading = true;
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
                        .find(|entry| entry.id == snapshot.agent)
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
        state().update(|s| s.loading = false);
        match result {
            Ok(Ok((entry, check))) => {
                state().update(|s| s.entry = Some(entry));
                apply_check(check);
            }
            Ok(Err(error)) => state().update(|s| s.error = error.to_string()),
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
    state().update(|s| {
        match methods {
            Ok(methods) => s.methods = methods,
            Err(error) => s.error = error.to_string(),
        }
        s.message = match check.status() {
            pb::AgentCheckStatus::Connected => tr!("auth-complete"),
            pb::AgentCheckStatus::AuthenticationRequired => tr!("auth-choose-method"),
            _ => check.message.clone(),
        };
        s.check = Some(check);
    });
}

pub(crate) fn connected() -> bool {
    state().with(|s| {
        !s.loading
            && !s.running
            && s.check
                .as_ref()
                .is_some_and(|c| c.status() == pb::AgentCheckStatus::Connected)
    })
}
pub(crate) fn cancel() {
    state().update(|s| {
        s.epoch += 1;
        s.loading = false;
        s.running = false;
        s.pending.clear();
        s.tx = None;
    });
}
pub(crate) fn stop() {
    cancel();
    state().update(|s| s.message = tr!("auth-cancelled"));
}

pub(crate) fn start(method: Method) {
    let snapshot = state().get_untracked();
    if snapshot.running || snapshot.loading || !snapshot.methods.contains(&method) {
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
        s.tx = Some(tx);
        s.running = true;
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
                finish(Some(error.message().to_owned()));
                return;
            }
            Err(error) => {
                finish(Some(error.to_string()));
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
                    finish(Some(tr!("auth-stream-ended")));
                    return;
                }
                Ok((_, Ok(Err(error)))) => {
                    finish(Some(error.message().to_owned()));
                    return;
                }
                Err(error) => {
                    finish(Some(error.to_string()));
                    return;
                }
            };
            match event.kind {
                Some(pb::authentication_event::Kind::Check(check)) => {
                    // Publish completion after the final check, not after a form response.
                    apply_check(check);
                    finish(None);
                    return;
                }
                Some(pb::authentication_event::Kind::Elicitation(pending)) => state().update(|s| {
                    s.pending.retain(|p| p.request_id != pending.request_id);
                    s.pending.push(pending);
                }),
                Some(pb::authentication_event::Kind::ElicitationCancelled(id)) => {
                    state().update(|s| s.pending.retain(|p| p.request_id != id));
                }
                Some(pb::authentication_event::Kind::ElicitationComplete(payload)) => {
                    if let Ok(value) = payload.decode::<serde_json::Value>() {
                        state().update(|s| s.pending.retain(|p| {
                            !request(p).is_ok_and(|r| matches!(r.mode, elicitation::Mode::Url(url) if value.get("elicitationId").and_then(|v| v.as_str()) == Some(url.elicitation_id.as_str())))
                        }));
                    }
                }
                Some(pb::authentication_event::Kind::Terminal(_)) => {
                    finish(Some(tr!("terminal-auth-required")));
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

fn finish(error: Option<String>) {
    state().update(|s| {
        s.running = false;
        s.tx = None;
        s.pending.clear();
        if let Some(error) = error {
            s.error = error;
            s.message.clear();
        }
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
        .pending
        .iter()
        .any(|p| p.request_id == pending.request_id && !p.answered)
    {
        return;
    }
    let Some(tx) = snapshot.tx else { return };
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
                if retain {
                    if let Some(p) = s.pending.iter_mut().find(|p| p.request_id == request_id) {
                        p.answered = true;
                    }
                } else {
                    s.pending.retain(|p| p.request_id != request_id);
                }
            }),
            Err(_) => finish(Some(tr!("auth-stream-ended"))),
        }
    });
}
