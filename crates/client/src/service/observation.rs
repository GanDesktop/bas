//! Converts protocol observations into the existing history presentation model.
//! These local records are never sent on the BAS management subscription.
use super::{convert::Error, pb};
use crate::{acp, model::*};

impl pb::AcpObservation {
    pub fn from_event(event: &Event) -> Option<Self> {
        let record = event.observation()?;
        Some(Self {
            session: event.session.to_string(),
            cursor: event.cursor.raw(),
            at: event.at.millis(),
            json: serde_json::to_vec(&record.message).expect("protocol JSON"),
            start: record.start.raw(),
            turn: record.turn.map(|turn| turn.to_string()),
        })
    }

    /// Decode one protocol message for incremental conversation rendering.
    ///
    /// # Errors
    /// Rejects malformed JSON-RPC, identifiers or history positions.
    pub fn history_events(&self) -> Result<Vec<Event>, Error> {
        let session: SessionId = self.session.parse()?;
        let message: serde_json::Value = serde_json::from_slice(&self.json)?;
        if message.get("jsonrpc").and_then(|value| value.as_str()) != Some("2.0") {
            return Err(Error::InvalidObservation);
        }
        let turn = self.turn.as_ref().map(|value| value.parse()).transpose()?;
        let mut kinds = Vec::new();
        match message.get("method").and_then(|value| value.as_str()) {
            Some("session/prompt") => {
                let request: acp::PromptRequest =
                    serde_json::from_value(message["params"].clone())?;
                let turn = turn.ok_or(Error::InvalidObservation)?;
                let request_id = message["id"].as_str().and_then(|id| id.parse().ok());
                kinds.push(EventKind::TurnStarted { turn, request_id });
                kinds.extend(request.prompt.into_iter().map(|content| {
                    EventKind::Acp(acp::SessionUpdate::UserMessageChunk(
                        acp::ContentChunk::new(content).meta(request.meta.clone()),
                    ))
                }));
            }
            Some("session/update") => {
                let notification: acp::SessionNotification =
                    serde_json::from_value(message["params"].clone())?;
                kinds.push(if notification.session_id.to_string() == self.session {
                    EventKind::Acp(notification.update)
                } else {
                    EventKind::Subagent(notification)
                });
            }
            None if turn.is_some() => {
                let result = if let Some(error) = message.get("error") {
                    Err(serde_json::from_value(error.clone())?)
                } else if let Some(result) = message.get("result") {
                    Ok(serde_json::from_value(result.clone())?)
                } else {
                    return Err(Error::InvalidObservation);
                };
                kinds.push(EventKind::TurnFinished {
                    turn: turn.expect("checked turn"),
                    result,
                });
            }
            _ => return Ok(Vec::new()),
        }
        if self.start.checked_add(kinds.len() as u64 - 1) != Some(self.cursor) {
            return Err(Error::InvalidObservation);
        }
        let is_prompt =
            message.get("method").and_then(|value| value.as_str()) == Some("session/prompt");
        let is_update =
            message.get("method").and_then(|value| value.as_str()) == Some("session/update");
        let mut events: Vec<_> = kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                let mut event =
                    Event::new(session, Cursor::from_raw(self.start + index as u64), kind);
                event.at = Timestamp::from_millis(self.at);
                event.protocol = None;
                event.historical_only = is_prompt;
                event
            })
            .collect();
        if !is_update && let Some(last) = events.last_mut() {
            last.protocol = Some(ProtocolRecord {
                message,
                start: Cursor::from_raw(self.start),
                turn,
            });
        }
        Ok(events)
    }
}
