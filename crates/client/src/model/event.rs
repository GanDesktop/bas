use super::{Cursor, SessionId, Timestamp, TurnId};
use crate::acp;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(test)]
mod tests;

/// The agent's native answer to an ACP prompt, never a daemon or storage failure.
pub type PromptResult = Result<acp::PromptResponse, acp::Error>;

/// Why Bas stopped tracking a prompt without receiving an ACP result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InterruptionReason {
    BackendFailure { message: String },
    DispatchFailure { message: String },
    SessionStopped,
    DaemonRestarted,
    CancelledBeforeDispatch,
}

/// ACP updates retain their complete schema; other variants describe daemon lifecycle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    Acp(
        #[serde(serialize_with = "serialize_acp", deserialize_with = "deserialize_acp")]
        acp::SessionUpdate,
    ),
    /// Child traffic stays scoped to its original ACP session, never flattened into the parent.
    Subagent(acp::SessionNotification),
    TurnStarted {
        turn: TurnId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<uuid::Uuid>,
    },
    TurnFinished {
        turn: TurnId,
        result: PromptResult,
    },
    TurnInterrupted {
        turn: TurnId,
        reason: InterruptionReason,
    },
    TitleChanged {
        title: String,
    },
    ArchiveChanged {
        archived: bool,
    },
    WorkspaceChanged {
        directories: Vec<std::path::PathBuf>,
        config: acp::NewSessionResponse,
        #[serde(default)]
        subagents: super::Subagents,
    },
    ActivityChanged {
        turn: Option<TurnId>,
    },
    StatusChanged {
        status: super::SessionStatus,
    },
    Failed {
        message: String,
    },
}

fn serialize_acp<S: Serializer>(
    update: &acp::SessionUpdate,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Payload<'a> {
        update: &'a acp::SessionUpdate,
    }
    Payload { update }.serialize(serializer)
}

fn deserialize_acp<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<acp::SessionUpdate, D::Error> {
    #[derive(Deserialize)]
    struct Payload {
        update: acp::SessionUpdate,
    }
    Ok(Payload::deserialize(deserializer)?.update)
}

/// Ordered, replayable session event. ACP chunks use the same cursor as lifecycle events.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub session: SessionId,
    pub cursor: Cursor,
    pub at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ProtocolRecord>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub historical_only: bool,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    /// Live notices have no replay cursor and never enter session history.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(&self.kind, EventKind::Acp(acp::SessionUpdate::Notice(_)))
            || matches!(&self.kind, EventKind::Subagent(notification) if matches!(notification.update, acp::SessionUpdate::Notice(_)))
    }

    #[must_use]
    pub fn new(session: SessionId, cursor: Cursor, kind: EventKind) -> Self {
        Self {
            session,
            cursor,
            at: Timestamp::now(),
            protocol: None,
            historical_only: false,
            kind,
        }
    }
}

/// Protocol data recorded with a history entry. Application state stays in `kind`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProtocolRecord {
    pub message: serde_json::Value,
    pub start: Cursor,
    pub turn: Option<TurnId>,
}

impl Event {
    /// Conversation events do not belong to the BAS management subscription.
    pub fn management(&self) -> Option<Self> {
        let kind = match &self.kind {
            EventKind::Acp(_) | EventKind::Subagent(_) => return None,
            EventKind::TurnStarted { turn, .. } => EventKind::ActivityChanged { turn: Some(*turn) },
            EventKind::TurnFinished { .. } => EventKind::ActivityChanged { turn: None },
            kind => kind.clone(),
        };
        Some(Self {
            session: self.session,
            cursor: self.cursor,
            at: self.at,
            protocol: None,
            historical_only: false,
            kind,
        })
    }
}

impl Event {
    /// Returns the protocol exchange represented by this history record.
    pub fn observation(&self) -> Option<ProtocolRecord> {
        if let Some(record) = &self.protocol {
            return Some(record.clone());
        }
        if self.historical_only {
            return None;
        }
        let params = match &self.kind {
            EventKind::Acp(update) => {
                serde_json::json!({"sessionId":self.session, "update":update})
            }
            EventKind::Subagent(notification) => {
                serde_json::to_value(notification).expect("ACP notification")
            }
            _ => return None,
        };
        Some(ProtocolRecord {
            message: serde_json::json!({"jsonrpc":"2.0", "method":"session/update", "params":params}),
            start: self.cursor,
            turn: None,
        })
    }
}
