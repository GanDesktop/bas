pub(crate) mod composer;
pub(crate) mod interactions;
pub(crate) mod management;
pub(crate) mod navigation;
pub(crate) mod profiles;
mod terminal;
pub(crate) mod timeline;

use crate::shared::{api, i18n::tr};
use bas_client::{
    acp,
    model::{Event, EventKind, PromptSubmission, PromptSubmissionOutcome},
    service::{Client, pb},
};
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

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthenticationRequired {
    pub agent: String,
    generation: u64,
    selection: u64,
    creating: bool,
}

#[derive(Clone, Default)]
pub(crate) struct State {
    pub addresses: Vec<String>,
    pub endpoint: String,
    pub connected: bool,
    pub loading: bool,
    pub error: String,
    pub authentication: Option<AuthenticationRequired>,
    pub workspaces: Vec<pb::Workspace>,
    pub sessions: Vec<pb::Session>,
    pub agents: Vec<pb::Agent>,
    pub agent_icons: std::collections::BTreeMap<String, String>,
    pub workspace: Option<String>,
    pub session: Option<String>,
    pub agent: Option<String>,
    pub draft_agent: Option<String>,
    pub messages: Vec<Message>,
    pub overview: Vec<pb::HistoryOverviewEntry>,
    pub terminals: std::collections::BTreeMap<String, terminal::Terminal>,
    pub draft: String,
    pub sending: bool,
    pub uncertain: bool,
    pub running: bool,
    pub drawer: bool,
    pub configuring: bool,
    pub attachments: Vec<composer::Attachment>,
    pub usage: Option<acp::UsageUpdate>,
    pub rename_session: Option<pb::Session>,
    pub session_search: String,
    pub title_draft: String,
    pub permissions: Vec<bas_client::model::PendingPermission>,
    pub elicitations: Vec<pb::PendingElicitation>,
    client: Option<Client>,
    owner: Option<Owner>,
    generation: u64,
    selection: u64,
    timeline_turn: u64,
    submitted_turn: Option<bas_client::model::TurnId>,
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
        s.draft_agent = value.as_ref().map(ToString::to_string);
        s.authentication = None;
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
        s.authentication = None;
        s.running = false;
        s.sending = false;
        s.loading = false;
        s.uncertain = false;
    });
}

pub(crate) fn disconnect() {
    api::replace(None);
    state().update(|s| {
        s.generation += 1;
        s.selection += 1;
        s.submitted_turn = None;
        s.authentication = None;
        s.client = None;
        s.connected = false;
        s.loading = false;
        s.running = false;
        s.uncertain |= s.sending;
        s.sending = false;
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
    api::replace(None);
    state().update(|s| {
        s.generation += 1;
        s.selection += 1;
        s.submitted_turn = None;
        s.authentication = None;
        s.loading = true;
        s.connected = false;
        s.drawer = false;
        s.client = None;
        s.error.clear();
        s.workspaces.clear();
        s.sessions.clear();
        s.agents.clear();
        s.agent_icons.clear();
        s.workspace = None;
        s.session = None;
        s.agent = None;
        s.messages.clear();
        s.overview.clear();
        s.terminals.clear();
        s.attachments.clear();
        s.permissions.clear();
        s.elicitations.clear();
        s.usage = None;
        s.configuring = false;
        s.draft.clear();
        s.sending = false;
        s.running = false;
        s.uncertain = false;
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
                api::replace(Some(client.clone()));
                management::load_icons();
                state().update(|s| {
                    s.client = Some(client);
                    s.connected = true;
                    s.drawer = crate::shared::routes::page().get_untracked()
                        == crate::shared::routes::Page::Conversation;
                    s.loading = false;
                    s.workspace = workspaces.first().map(|w| w.id.clone());
                    s.agent = agents.first().map(|a| a.id.clone());
                    s.draft_agent = s.agent.clone();
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
            Ok(Err(error)) => fail(error.to_string()),
            Err(error) => fail(error.to_string()),
        }
    });
}

fn fail(error: String) {
    state().update(|s| {
        s.error = error;
        s.loading = false;
        s.sending = false;
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
        s.authentication = None;
        s.session = Some(id.clone());
        if let Some(session) = s.sessions.iter().find(|session| session.id == id) {
            s.agent = Some(session.agent.clone());
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
        s.sending = false;
        s.uncertain = false;
        s.running = s
            .sessions
            .iter()
            .find(|session| session.id == id)
            .is_some_and(|session| session.active_turn.is_some());
    });
    observe();
    interactions::observe();
}

pub(crate) fn refresh() {
    state().update(|s| {
        s.selection += 1;
        s.submitted_turn = None;
        s.authentication = None;
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
    let (Some(mut client), Some(id)) = (s.client, s.session) else {
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
                            s.running = session.active_turn.is_some();
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
    state().update(|s| match event.kind {
        EventKind::Acp(update) => {
            let (role, chunk) = match update {
                acp::SessionUpdate::UserMessageChunk(chunk) => ("user", chunk),
                acp::SessionUpdate::AgentMessageChunk(chunk) => ("assistant", chunk),
                acp::SessionUpdate::AgentThoughtChunk(chunk) => ("thought", chunk),
                acp::SessionUpdate::UsageUpdate(usage) => {
                    s.usage = Some(usage);
                    return;
                }
                acp::SessionUpdate::ToolCallUpdate(update) => {
                    terminal::apply(&mut s.terminals, update.meta.as_ref());
                    if let Some(message) = s
                        .messages
                        .iter_mut()
                        .find(|m| m.key == update.tool_call_id.to_string() && m.tool.is_some())
                        && let Some(tool) = &mut message.tool
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
            s.running = true;
            s.timeline_turn += 1;
        }
        EventKind::TurnFinished { turn, result } => {
            s.running = false;
            s.timeline_turn += 1;
            if let Err(error) = result {
                if error.code == acp::ErrorCode::AuthRequired
                    && (following || s.submitted_turn == Some(turn))
                {
                    s.error.clear();
                    if let Some(agent) = s.agent.clone() {
                        s.authentication = Some(AuthenticationRequired {
                            agent,
                            creating: false,
                            generation: s.generation,
                            selection: s.selection,
                        });
                    }
                } else if error.code != acp::ErrorCode::AuthRequired {
                    s.error = error.message;
                }
            }
        }
        EventKind::TurnInterrupted { .. } => {
            s.running = false;
            s.timeline_turn += 1;
        }
        EventKind::Failed { message } => s.error = message,
        _ => {}
    });
}

pub(crate) fn create_session() {
    create_session_with_draft(false);
}

fn create_session_with_draft(preserve_draft: bool) {
    let s = state().get_untracked();
    let (Some(mut client), Some(workspace_id), Some(agent)) =
        (s.client, s.workspace, s.draft_agent)
    else {
        return;
    };
    if s.loading {
        return;
    }
    state().update(|s| {
        s.loading = true;
        s.error.clear();
    });
    state().update(|s| s.authentication = None);
    let requested_agent = agent.clone();
    task(async move {
        let result = background(async move {
            client
                .create_session(pb::CreateSessionRequest {
                    workspace_id: Some(workspace_id),
                    agent,
                    ..Default::default()
                })
                .await
        })
        .await;
        if !current(s.generation, s.selection) {
            return;
        }
        state().update(|s| s.loading = false);
        match result {
            Ok(Ok(response)) => {
                if let Some(session) = response.into_inner().session {
                    let id = session.id.clone();
                    state().update(|s| s.sessions.insert(0, session));
                    select_session(id);
                    if preserve_draft {
                        state().update(|state| {
                            state.draft = s.draft;
                            state.attachments = s.attachments;
                        });
                    }
                }
            }
            Ok(Err(error)) if authentication_required(&error) => {
                require_authentication(requested_agent, true);
            }
            Ok(Err(error)) => fail(error.message().to_owned()),
            other => fail(format!("{other:?}")),
        }
    });
}

pub(crate) fn send() {
    let s = state().get_untracked();
    if s.configuring
        || s.sending
        || s.running
        || s.uncertain
        || (s.draft.trim().is_empty() && s.attachments.is_empty())
    {
        return;
    }
    let (Some(mut client), Some(id)) = (s.client, s.session) else {
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
        s.sending = true;
        s.submitted_turn = Some(submission.turn);
        s.error.clear();
    });
    task(async move {
        let result = background(async move { client.submit_prompt(&submission).await }).await;
        if !current(s.generation, s.selection) {
            return;
        }
        state().update(|s| s.sending = false);
        match result {
            Ok(Ok(PromptSubmissionOutcome::Accepted(_))) => state().update(|s| {
                s.draft.clear();
                s.attachments.clear();
                s.running = true;
            }),
            Ok(Ok(PromptSubmissionOutcome::Rejected(error))) => fail(error.to_string()),
            Ok(Err(error)) if authentication_required(&error) => {
                if let Some(agent) = s.agent {
                    require_authentication(agent, false);
                }
            }
            _ => {
                state().update(|s| s.uncertain = true);
                fail(tr!("uncertain"));
            }
        }
    });
}

pub(crate) fn cancel() {
    let s = state().get_untracked();
    let (Some(mut client), Some(id)) = (s.client, s.session) else {
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

fn require_authentication(agent: String, creating: bool) {
    state().update(|s| {
        s.error.clear();
        s.drawer = false;
        s.authentication = Some(AuthenticationRequired {
            agent,
            creating,
            generation: s.generation,
            selection: s.selection,
        });
    });
}

pub(crate) fn authentication_completed(agent: &str) {
    let Some(required) = state().get_untracked().authentication else {
        return;
    };
    if required.agent != agent || !current(required.generation, required.selection) {
        return;
    }
    state().update(|s| {
        s.authentication = None;
        s.error.clear();
    });
    if required.creating {
        create_session_with_draft(true);
    }
}
