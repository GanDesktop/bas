pub(crate) mod composer;
pub(crate) mod interactions;
pub(crate) mod management;
mod model;
pub(crate) mod navigation;
mod outcome;
pub(crate) mod profiles;
mod terminal;
pub(crate) mod timeline;

use crate::{
    entities::agent::{self, Key, LoginStatus},
    shared::{api, i18n::tr},
};
use bas_client::{
    acp,
    model::{Event, EventKind, InterruptionReason, PromptSubmission, PromptSubmissionOutcome},
    service::{Client, pb},
};
pub(crate) use model::*;
use nsh_core::{prelude::*, storage::Storage};
use std::time::Duration;

const CONNECTIONS_KEY: &str = "bas.connections";
const LAST_KEY: &str = "bas.last-connection";
const HISTORY_BATCH: u32 = 256;
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
pub(crate) struct Message {
    pub sequence: u64,
    pub turn: u64,
    pub role: String,
    pub body: String,
    pub tool: Option<acp::ToolCall>,
    pub media: Option<acp::ContentBlock>,
    key: String,
}

#[derive(Clone, Default)]
pub(crate) struct State {
    pub addresses: Vec<String>,
    pub endpoint: String,
    pub backend: Backend,
    pub creation: Creation,
    pub execution: Execution,
    pub error: String,
    pub login_request: Option<LoginRequest>,
    pub workspaces: Vec<pb::Workspace>,
    pub sessions: Vec<pb::Session>,
    pub agents: Vec<pb::Agent>,
    pub agent_icons: std::collections::BTreeMap<String, String>,
    pub workspace: Option<String>,
    pub session: Option<String>,
    pub draft_agent: Option<String>,
    pub messages: Vec<Message>,
    pub overview: Vec<pb::HistoryOverviewEntry>,
    pub terminals: std::collections::BTreeMap<String, terminal::Terminal>,
    pub draft: String,
    pub drawer: bool,
    pub configuring: bool,
    pub attachments: Vec<composer::Attachment>,
    pub usage: Option<acp::UsageUpdate>,
    pub rename_session: Option<pb::Session>,
    pub session_search: String,
    pub title_draft: String,
    pub permissions: Vec<bas_client::model::PendingPermission>,
    pub elicitations: Vec<pb::PendingElicitation>,
    owner: Option<Owner>,
    generation: u64,
    selection: u64,
    timeline_turn: u64,
    submitted_turn: Option<bas_client::model::TurnId>,
}

impl State {
    pub fn active_agent(&self) -> Option<&str> {
        self.sessions
            .iter()
            .find(|session| Some(&session.id) == self.session.as_ref())
            .map(|session| session.agent.as_str())
    }

    pub fn login_agent(&self) -> Option<String> {
        let agent = self.active_agent().or_else(|| {
            if self.session.is_none() {
                self.draft_agent.as_deref()
            } else {
                None
            }
        })?;
        (Key::new(self.backend.endpoint()?, agent).status() == LoginStatus::Required)
            .then(|| agent.to_owned())
    }
}

pub(crate) fn state() -> Signal<State> {
    global::<State>()
}

pub(crate) fn initialize() {
    profiles::initialize();
    navigation::initialize();
    let addresses = Storage::get_item(CONNECTIONS_KEY)
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let endpoint = std::env::var("BAS_MOBILE_ENDPOINT")
        .ok()
        .or_else(|| Storage::get_item(LAST_KEY))
        .unwrap_or_default();
    state().update(|s| {
        s.owner = Some(Owner::current());
        s.addresses = addresses;
        s.endpoint = endpoint;
        s.drawer = false;
    });
    if !state().get_untracked().endpoint.is_empty() {
        connect();
    }
}

// Network work belongs to the app, including requests started by recycled rows.
fn task(future: impl std::future::Future<Output = ()> + 'static) {
    let owner = state()
        .get_untracked()
        .owner
        .expect("app initialized before actions");
    owner.run(|| {
        spawn(future);
    });
}

pub(crate) fn edit_endpoint(value: &str) {
    state().update(|s| s.endpoint = value.to_owned());
}
pub(crate) fn edit_draft(value: &str) {
    state().update(|s| s.draft = value.to_owned());
}
pub(crate) fn open_drawer() {
    dismiss_keyboard();
    state().update(|s| s.drawer = true);
}
pub(crate) fn close_drawer() {
    state().update(|s| s.drawer = false);
}
pub(crate) fn choose_backend(value: &Option<SharedString>) {
    if let Some(value) = value {
        edit_endpoint(value);
        connect();
    }
}
pub(crate) fn choose_agent(value: &Option<SharedString>) {
    state().update(|s| {
        if s.creation.creating() {
            return;
        }
        s.draft_agent = value.as_ref().map(ToString::to_string);
        s.login_request = None;
        s.creation = Creation::Idle;
    });
}
pub(crate) fn choose_workspace(value: &Option<SharedString>) {
    state().update(|s| {
        s.workspace = value.as_ref().map(ToString::to_string);
        s.session = None;
        s.messages.clear();
        s.overview.clear();
        s.terminals.clear();
        s.attachments.clear();
        s.permissions.clear();
        s.elicitations.clear();
        s.usage = None;
        s.configuring = false;
        s.draft.clear();
        s.selection += 1;
        s.submitted_turn = None;
        s.login_request = None;
        s.creation = Creation::Idle;
        s.execution = Execution::Idle;
    });
}

pub(crate) fn disconnect() {
    let endpoint = state()
        .get_untracked()
        .backend
        .endpoint()
        .unwrap_or_default()
        .to_owned();
    api::replace(None, endpoint.clone());
    agent::invalidate_backend(&endpoint);
    state().update(|s| {
        s.generation += 1;
        s.selection += 1;
        s.submitted_turn = None;
        s.login_request = None;
        s.backend = Backend::Disconnected;
        s.creation = Creation::Idle;
        s.execution = if matches!(s.execution, Execution::Submitting | Execution::Uncertain) {
            Execution::Uncertain
        } else {
            Execution::Idle
        };
        s.permissions.clear();
        s.elicitations.clear();
        s.error.clear();
    });
}

pub(crate) fn connect() {
    let endpoint = state().get_untracked().endpoint.trim().to_owned();
    if endpoint.is_empty() {
        return;
    }
    api::replace(None, endpoint.clone());
    agent::invalidate_backend(&endpoint);
    state().update(|s| {
        s.generation += 1;
        s.selection += 1;
        s.submitted_turn = None;
        s.login_request = None;
        s.backend = Backend::Connecting;
        s.creation = Creation::Idle;
        s.drawer = false;
        s.error.clear();
        s.workspaces.clear();
        s.sessions.clear();
        s.agents.clear();
        s.agent_icons.clear();
        s.workspace = None;
        s.session = None;
        s.messages.clear();
        s.overview.clear();
        s.terminals.clear();
        s.attachments.clear();
        s.permissions.clear();
        s.elicitations.clear();
        s.usage = None;
        s.configuring = false;
        s.draft.clear();
        s.execution = Execution::Idle;
    });
    let generation = state().get_untracked().generation;
    task(async move {
        let saved = endpoint.clone();
        let result = background(async move {
            let mut client = if endpoint.starts_with("http://") {
                Client::connect(endpoint).await?.with_payload_limit()
            } else {
                Client::connect_target(&endpoint.parse()?).await?
            };
            let workspaces = client
                .list_workspaces(pb::ListWorkspacesRequest::default())
                .await?
                .into_inner()
                .workspaces;
            let sessions = client
                .list_sessions(pb::ListSessionsRequest {
                    include_archived: true,
                })
                .await?
                .into_inner()
                .sessions;
            let agents = client
                .list_agents(pb::ListAgentsRequest::default())
                .await?
                .into_inner()
                .agents;
            anyhow::Ok((client, workspaces, sessions, agents))
        })
        .await;
        if state().get_untracked().generation != generation {
            return;
        }
        match result {
            Ok(Ok((client, workspaces, sessions, agents))) => {
                api::replace(Some(client.clone()), saved.clone());
                management::load_icons();
                state().update(|s| {
                    s.backend = Backend::Connected {
                        client,
                        endpoint: saved.clone(),
                    };
                    s.drawer = crate::shared::routes::page().get_untracked()
                        == crate::shared::routes::Page::Conversation;
                    s.workspace = workspaces.first().map(|w| w.id.clone());
                    s.draft_agent = agents.first().map(|a| a.id.clone());
                    s.workspaces = workspaces;
                    s.sessions = sessions;
                    s.agents = agents;
                    if !s.addresses.contains(&saved) {
                        s.addresses.push(saved.clone());
                    }
                });
                let json = serde_json::to_string(&state().get_untracked().addresses)
                    .expect("string list serializes");
                if let Err(error) = Storage::set_item(CONNECTIONS_KEY, json)
                    .and_then(|_| Storage::set_item(LAST_KEY, saved))
                {
                    fail(error.to_string());
                }
            }
            Ok(Err(error)) => state().update(|s| s.backend = Backend::Failed(error.to_string())),
            Err(error) => state().update(|s| s.backend = Backend::Failed(error.to_string())),
        }
    });
}

fn fail(error: String) {
    state().update(|s| {
        s.error = error;
    });
}
fn current(generation: u64, selection: u64) -> bool {
    state().with(|s| s.generation == generation && s.selection == selection)
}

pub(crate) fn sessions() -> Vec<pb::Session> {
    state().with(|s| {
        s.sessions
            .iter()
            .filter(|session| Some(&session.workspace_id) == s.workspace.as_ref())
            .cloned()
            .collect()
    })
}

pub(crate) fn select_session(id: String) {
    state().update(|s| {
        s.selection += 1;
        s.submitted_turn = None;
        s.login_request = None;
        s.creation = Creation::Idle;
        s.session = Some(id.clone());
        if let Some(session) = s.sessions.iter().find(|session| session.id == id) {
            s.workspace = Some(session.workspace_id.clone());
        }
        s.messages.clear();
        s.overview.clear();
        s.terminals.clear();
        s.attachments.clear();
        s.permissions.clear();
        s.elicitations.clear();
        s.usage = None;
        s.configuring = false;
        s.draft.clear();
        s.drawer = false;
        s.error.clear();
        s.execution = if s
            .sessions
            .iter()
            .find(|session| session.id == id)
            .is_some_and(|session| session.active_turn.is_some())
        {
            Execution::Running
        } else {
            Execution::Idle
        };
    });
    observe();
    interactions::observe();
}

pub(crate) fn refresh() {
    state().update(|s| {
        s.selection += 1;
        s.submitted_turn = None;
        s.login_request = None;
        s.creation = Creation::Idle;
        s.messages.clear();
        s.overview.clear();
        s.terminals.clear();
        s.permissions.clear();
        s.elicitations.clear();
        s.usage = None;
        s.configuring = false;
        s.error.clear();
    });
    observe();
    interactions::observe();
}

// Read committed events incrementally so reconnecting never replays a prompt.
fn observe() {
    let s = state().get_untracked();
    let (Some(mut client), Some(id)) = (s.backend.client(), s.session) else {
        return;
    };
    task(async move {
        let mut after = None;
        let mut following = false;
        while current(s.generation, s.selection) {
            let mut rpc = client.clone();
            let session = id.clone();
            let result = background(async move {
                tokio::time::timeout(
                    RPC_TIMEOUT,
                    rpc.load_history(pb::LoadHistoryRequest {
                        session,
                        after,
                        limit: HISTORY_BATCH,
                    }),
                )
                .await
            })
            .await;
            if !current(s.generation, s.selection) {
                break;
            }
            match result {
                Ok(Ok(Ok(response))) => {
                    let page = response.into_inner();
                    let changed = !page.events.is_empty();
                    for raw in page.events {
                        let event: Result<Event, _> = raw.try_into();
                        match event {
                            Ok(event) => {
                                after = Some(event.cursor.raw());
                                apply(event, following);
                            }
                            Err(error) => {
                                fail(format!("{error}"));
                                return;
                            }
                        }
                    }
                    if page.next.is_some() {
                        continue;
                    }
                    following = true;
                    if changed {
                        navigation::load_overview();
                    }
                }
                other => {
                    fail(format!("{other:?}"));
                    break;
                }
            }
            // Metadata conveys execution state even when no new text arrives.
            let session = id.clone();
            let result = background(async move {
                let result = tokio::time::timeout(
                    RPC_TIMEOUT,
                    client.get_session(pb::GetSessionRequest { session }),
                )
                .await;
                (client, result)
            })
            .await;
            if !current(s.generation, s.selection) {
                break;
            }
            match result {
                Ok((next, Ok(Ok(response)))) => {
                    client = next;
                    if let Some(session) = response.into_inner().session {
                        state().update(|s| {
                            s.execution.observe(session.active_turn.is_some());
                            if let Some(old) =
                                s.sessions.iter_mut().find(|old| old.id == session.id)
                            {
                                *old = session;
                            }
                        });
                    }
                }
                other => {
                    fail(format!("{other:?}"));
                    break;
                }
            }
            timer(POLL_INTERVAL).await;
        }
    });
}

fn apply(event: Event, following: bool) {
    let sequence = event.cursor.raw();
    let mut readiness = None;
    batch(|| {
        state().update(|s| match event.kind {
            EventKind::Acp(update) => {
                let (role, chunk) =
                    match update {
                        acp::SessionUpdate::UserMessageChunk(chunk) => ("user", chunk),
                        acp::SessionUpdate::AgentMessageChunk(chunk) => ("assistant", chunk),
                        acp::SessionUpdate::AgentThoughtChunk(chunk) => ("thought", chunk),
                        acp::SessionUpdate::UsageUpdate(usage) => {
                            s.usage = Some(usage);
                            return;
                        }
                        acp::SessionUpdate::ToolCallUpdate(update) => {
                            terminal::apply(&mut s.terminals, update.meta.as_ref());
                            if let Some(message) = s.messages.iter_mut().find(|m| {
                                m.key == update.tool_call_id.to_string() && m.tool.is_some()
                            }) && let Some(tool) = &mut message.tool
                            {
                                tool.update(update.fields);
                                message.body = tool.title.clone();
                            }
                            return;
                        }
                        acp::SessionUpdate::ToolCall(tool) => {
                            terminal::apply(&mut s.terminals, tool.meta.as_ref());
                            s.messages.push(Message {
                                sequence,
                                turn: s.timeline_turn,
                                role: "agent".into(),
                                body: tool.title.clone(),
                                key: tool.tool_call_id.to_string(),
                                tool: Some(tool),
                                media: None,
                            });
                            return;
                        }
                        _ => return,
                    };
                let body = match chunk.content {
                    acp::ContentBlock::Text(text) => text.text,
                    content => {
                        s.messages.push(Message {
                            sequence,
                            turn: s.timeline_turn,
                            role: role.into(),
                            media: Some(content),
                            ..Default::default()
                        });
                        return;
                    }
                };
                let key = chunk
                    .message_id
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                if let Some(last) = s.messages.last_mut().filter(|m| {
                    m.role == role
                        && m.key == key
                        && m.turn == s.timeline_turn
                        && m.media.is_none()
                        && m.tool.is_none()
                }) {
                    last.body.push_str(&body);
                } else {
                    s.messages.push(Message {
                        sequence,
                        turn: s.timeline_turn,
                        role: role.into(),
                        body,
                        key,
                        ..Default::default()
                    });
                }
            }
            EventKind::TurnStarted { .. } => {
                s.error.clear();
                s.execution.observe(true);
                s.timeline_turn += 1;
            }
            EventKind::TurnFinished { turn, result } => {
                if !s.execution.submitting()
                    && (s.submitted_turn.is_none() || s.submitted_turn == Some(turn))
                {
                    s.execution = Execution::Idle;
                }
                s.timeline_turn += 1;
                if let Err(error) = result {
                    if error.code == acp::ErrorCode::AuthRequired
                        && (following || s.submitted_turn == Some(turn))
                    {
                        s.error.clear();
                        if let (Some(agent), Some(id)) =
                            (s.active_agent().map(str::to_owned), s.session.clone())
                        {
                            readiness = Some((
                                Key::new(s.backend.endpoint().unwrap_or_default(), &agent),
                                LoginStatus::Required,
                            ));
                            s.login_request = Some(LoginRequest {
                                target: LoginTarget::Session { agent, id },
                                generation: s.generation,
                                selection: s.selection,
                            });
                        }
                    } else if error.code != acp::ErrorCode::AuthRequired {
                        s.error = error.message;
                    }
                } else if let Ok(response) = result {
                    if let Some(message) = outcome::feedback(&response) {
                        s.error = message;
                    } else if (following || s.submitted_turn == Some(turn))
                        && let Some(agent) = s.active_agent()
                    {
                        readiness = Some((
                            Key::new(s.backend.endpoint().unwrap_or_default(), agent),
                            LoginStatus::Ready,
                        ));
                    }
                }
                if s.submitted_turn == Some(turn) {
                    s.submitted_turn = None;
                }
            }
            EventKind::TurnInterrupted { turn, reason } => {
                s.error = match reason {
                    InterruptionReason::BackendFailure { message }
                    | InterruptionReason::DispatchFailure { message } => message,
                    InterruptionReason::SessionStopped => tr!("turn-session-stopped"),
                    InterruptionReason::DaemonRestarted => tr!("turn-service-restarted"),
                    InterruptionReason::CancelledBeforeDispatch => {
                        tr!("turn-cancelled-before-dispatch")
                    }
                };
                if s.submitted_turn.is_none() || s.submitted_turn == Some(turn) {
                    if !s.execution.submitting() {
                        s.execution = Execution::Idle;
                    }
                    s.submitted_turn = None;
                }
                s.timeline_turn += 1;
            }
            EventKind::Failed { message } => s.error = message,
            _ => {}
        });
        if let Some((key, status)) = readiness {
            key.record(status);
        }
    });
}

pub(crate) fn create_session() {
    let s = state().get_untracked();
    let (Some(workspace), Some(agent)) = (s.workspace, s.draft_agent) else {
        return;
    };
    if !s.backend.connected() || s.creation.creating() {
        return;
    }
    create_session_request(CreateSession {
        agent,
        workspace,
        generation: s.generation,
        selection: s.selection,
    });
}

fn create_session_request(request: CreateSession) {
    let s = state().get_untracked();
    let Some(mut client) = s.backend.client() else {
        return;
    };
    let draft = s.session.is_none().then_some((s.draft, s.attachments));
    state().update(|s| {
        s.creation = Creation::Creating(request.clone());
        s.login_request = None;
    });
    task(async move {
        let target = request.clone();
        let result = background(async move {
            client
                .create_session(pb::CreateSessionRequest {
                    workspace_id: Some(target.workspace),
                    agent: target.agent,
                    ..Default::default()
                })
                .await
        })
        .await;
        if !current(request.generation, request.selection)
            || !state()
                .with(|s| matches!(&s.creation, Creation::Creating(active) if active == &request))
        {
            return;
        }
        match result {
            Ok(Ok(response)) => {
                if let Some(session) = response.into_inner().session {
                    Key::new(
                        s.backend.endpoint().expect("connected backend"),
                        &request.agent,
                    )
                    .record(LoginStatus::Ready);
                    let id = session.id.clone();
                    state().update(|s| s.sessions.insert(0, session));
                    select_session(id);
                    if let Some((draft, attachments)) = draft {
                        state().update(|s| {
                            s.draft = draft;
                            s.attachments = attachments;
                        });
                    }
                } else {
                    state().update(|s| {
                        s.creation = Creation::Failed {
                            request,
                            error: tr!("missing-session"),
                        }
                    });
                }
            }
            Ok(Err(error)) if authentication_required(&error) => {
                state().update(|s| s.creation = Creation::AuthenticationRequired(request.clone()));
                require_authentication(LoginTarget::Create(request));
            }
            Ok(Err(error)) => state().update(|s| {
                s.creation = Creation::Failed {
                    request,
                    error: error.message().to_owned(),
                }
            }),
            Err(error) => state().update(|s| {
                s.creation = Creation::Failed {
                    request,
                    error: error.to_string(),
                }
            }),
        }
    });
}

pub(crate) fn send() {
    let s = state().get_untracked();
    if s.configuring
        || !s.execution.can_send()
        || s.login_agent().is_some()
        || (s.draft.trim().is_empty() && s.attachments.is_empty())
    {
        return;
    }
    let agent = s.active_agent().map(str::to_owned);
    let (Some(mut client), Some(id)) = (s.backend.client(), s.session) else {
        return;
    };
    let mut content: Vec<_> = s.attachments.iter().map(|a| a.content.clone()).collect();
    if !s.draft.trim().is_empty() {
        content.push(acp::ContentBlock::Text(acp::TextContent::new(
            s.draft.clone(),
        )));
    }
    let submission = PromptSubmission::new(acp::PromptRequest::new(id, content));
    state().update(|s| {
        s.execution = Execution::Submitting;
        s.submitted_turn = Some(submission.turn);
        s.error.clear();
    });
    task(async move {
        let result = background(async move { client.submit_prompt(&submission).await }).await;
        if !current(s.generation, s.selection) {
            return;
        }
        match result {
            Ok(Ok(PromptSubmissionOutcome::Accepted(_))) => state().update(|s| {
                s.draft.clear();
                s.attachments.clear();
                if s.execution.submitting() {
                    // History can finish the turn before its acceptance receipt arrives.
                    s.execution = if s.submitted_turn.is_some() {
                        Execution::Running
                    } else {
                        Execution::Idle
                    };
                }
            }),
            Ok(Ok(PromptSubmissionOutcome::Rejected(error))) => {
                state().update(|s| {
                    s.execution = Execution::Idle;
                    s.submitted_turn = None;
                });
                fail(error.to_string());
            }
            Ok(Err(error)) if authentication_required(&error) => {
                state().update(|s| {
                    s.execution = Execution::Idle;
                    s.submitted_turn = None;
                });
                if let Some(agent) = agent {
                    require_authentication(LoginTarget::Session {
                        agent,
                        id: state().get_untracked().session.expect("current session"),
                    });
                }
            }
            _ => {
                state().update(|s| {
                    if s.submitted_turn.is_some() {
                        s.execution = Execution::Uncertain;
                        s.error = tr!("uncertain");
                    } else {
                        s.execution = Execution::Idle;
                        s.draft.clear();
                        s.attachments.clear();
                    }
                });
            }
        }
    });
}

pub(crate) fn cancel() {
    let s = state().get_untracked();
    let (Some(mut client), Some(id)) = (s.backend.client(), s.session) else {
        return;
    };
    task(async move {
        let result =
            background(async move { client.cancel_acp(&acp::CancelNotification::new(id)).await })
                .await;
        if current(s.generation, s.selection) {
            match result {
                Ok(Ok(())) => {}
                other => fail(format!("{other:?}")),
            }
        }
    });
}

// Only native ACP details distinguish agent login from backend access denial.
fn authentication_required(error: &tonic::Status) -> bool {
    error.code() == tonic::Code::Unauthenticated
        && serde_json::from_slice::<acp::Error>(error.details())
            .is_ok_and(|error| error.code == acp::ErrorCode::AuthRequired)
}

fn require_authentication(target: LoginTarget) {
    let Some(context) = api::context() else {
        return;
    };
    let key = Key::new(context.backend, target.agent());
    batch(|| {
        key.record(LoginStatus::Required);
        state().update(|s| {
            s.error.clear();
            s.drawer = false;
            s.login_request = Some(LoginRequest {
                target,
                generation: s.generation,
                selection: s.selection,
            });
        });
    });
}

pub(crate) fn authentication_completed(agent: &str) {
    let Some(required) = state().get_untracked().login_request else {
        return;
    };
    if required.target.agent() != agent || !current(required.generation, required.selection) {
        return;
    }
    state().update(|s| {
        s.login_request = None;
        s.error.clear();
    });
    if let LoginTarget::Create(request) = required.target {
        if state().with(|s| matches!(&s.creation, Creation::AuthenticationRequired(active) if active == &request)) {
            create_session_request(request);
        }
    }
}
