use crate::model::{Event, SessionId, SessionSnapshot};
use serde::{Deserialize, Serialize};

/// A safe recent-first range of the canonical event log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryWindow {
    pub session: SessionSnapshot,
    pub events: Vec<Event>,
    /// Earlier global state; replay in order before `events`. Replace this
    /// context with the older window's context when prepending, rather than
    /// retaining newer context ahead of older transcript events.
    pub context: Vec<Event>,
    pub before: Option<u64>,
    /// Attach cursor for the newest window, not an acknowledgement of older pages.
    pub cursor: u64,
}

/// Complete prompt navigation metadata, independent of loaded body windows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryOverview {
    pub session: SessionId,
    pub cursor: u64,
    pub entries: Vec<HistoryOverviewEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryOverviewEntry {
    pub cursor: u64,
    pub timestamp_millis: i64,
    pub preview: String,
}

/// A directly addressed logical page with dependency-complete older context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryLocation {
    pub window: HistoryWindow,
    /// Next logical page start; pass to `history_window_at` to move forwards.
    pub after: Option<u64>,
}

use super::pb;
use tonic::Status;

impl HistoryOverview {
    /// # Errors
    /// Rejects foreign sessions, unordered cursors, and invalid overview entries.
    pub fn decode(
        id: SessionId,
        response: pb::LoadHistoryOverviewResponse,
    ) -> Result<Self, Status> {
        if response.session.parse::<SessionId>().ok() != Some(id) {
            return Err(Status::data_loss(
                "history overview belongs to another session",
            ));
        }
        let mut previous = 0;
        let mut entries = Vec::with_capacity(response.entries.len());
        for entry in response.entries {
            if entry.cursor <= previous
                || entry.cursor > response.cursor
                || entry.timestamp_millis < 0
                || entry.preview.chars().count() > crate::model::history::PROMPT_PREVIEW_CHARS
            {
                return Err(Status::data_loss("invalid history overview entry"));
            }
            previous = entry.cursor;
            entries.push(HistoryOverviewEntry {
                cursor: entry.cursor,
                timestamp_millis: entry.timestamp_millis,
                preview: entry.preview,
            });
        }
        Ok(Self {
            session: id,
            cursor: response.cursor,
            entries,
        })
    }
}

impl HistoryLocation {
    /// # Errors
    /// Rejects malformed windows or navigation bounds that exclude the requested turn.
    pub fn decode(
        id: SessionId,
        requested: u64,
        response: pb::LoadHistoryWindowAtResponse,
    ) -> Result<Self, Status> {
        let raw = response
            .window
            .ok_or_else(|| Status::data_loss("missing history window"))?;
        if response
            .after
            .is_some_and(|after| after <= requested || after > raw.cursor)
        {
            return Err(Status::data_loss("invalid next history page cursor"));
        }
        let window = HistoryWindow::decode(id, response.after, raw)?;
        if requested == 0
            || window
                .events
                .first()
                .is_none_or(|event| event.cursor.raw() > requested)
            || window
                .events
                .last()
                .is_none_or(|event| event.cursor.raw() < requested)
            || window.before
                != window
                    .events
                    .first()
                    .filter(|event| event.cursor.raw() > 1)
                    .map(|event| event.cursor.raw())
        {
            return Err(Status::data_loss(
                "history window does not contain requested cursor",
            ));
        }
        Ok(Self {
            window,
            after: response.after,
        })
    }
}

fn decode_session(id: SessionId, session: Option<pb::Session>) -> Result<SessionSnapshot, Status> {
    let session: SessionSnapshot = session
        .ok_or_else(|| Status::data_loss("history response is missing session metadata"))?
        .try_into()
        .map_err(invalid_payload)?;
    if session.meta.id != id {
        return Err(Status::data_loss(
            "history metadata belongs to another session",
        ));
    }
    Ok(session)
}

fn invalid_payload(error: crate::protocol::Error) -> Status {
    Status::data_loss(error.to_string())
}

fn decode_events(
    id: SessionId,
    events: Vec<pb::Event>,
    contiguous: bool,
) -> Result<Vec<Event>, Status> {
    let mut previous: Option<u64> = None;
    events
        .into_iter()
        .map(|event| {
            let event: Event = event.try_into().map_err(invalid_payload)?;
            let cursor = event.cursor.raw();
            if event.session != id || cursor == 0 || event.is_transient() {
                return Err(Status::data_loss(
                    "invalid history event identity or cursor",
                ));
            }
            if previous.is_some_and(|previous| {
                cursor <= previous || (contiguous && previous.checked_add(1) != Some(cursor))
            }) {
                return Err(Status::data_loss("history events are not in cursor order"));
            }
            previous = Some(cursor);
            Ok(event)
        })
        .collect()
}

impl HistoryWindow {
    /// # Errors
    /// Rejects malformed snapshots, events, dependencies, or inconsistent page bounds.
    pub fn decode(
        id: SessionId,
        requested_before: Option<u64>,
        response: pb::LoadHistoryWindowResponse,
    ) -> Result<Self, Status> {
        let session = decode_session(id, response.session)?;
        if response.cursor != session.meta.revision.raw() {
            return Err(Status::data_loss("history cursor disagrees with snapshot"));
        }
        let events = decode_events(id, response.events, true)?;
        let context = decode_events(id, response.context, false)?;
        let upper = requested_before.map_or(response.cursor, |before| before.saturating_sub(1));
        if requested_before.is_some_and(|before| before > response.cursor.saturating_add(1))
            || events
                .last()
                .is_some_and(|event| event.cursor.raw() != upper)
            || (events.is_empty() && upper != 0)
        {
            return Err(Status::data_loss(
                "history window does not end at its requested cursor",
            ));
        }
        if let Some(before) = response.before
            && (before <= 1
                || events.first().map(|event| event.cursor.raw()) != Some(before)
                || requested_before.is_some_and(|requested| before >= requested))
        {
            return Err(Status::data_loss(
                "history returned an invalid older cursor",
            ));
        }
        if context.last().is_some_and(|event| {
            event.cursor.raw() >= events.first().map_or(0, |event| event.cursor.raw())
        }) {
            return Err(Status::data_loss(
                "history context overlaps the event window",
            ));
        }
        Ok(Self {
            session,
            events,
            context,
            before: response.before,
            cursor: response.cursor,
        })
    }
}
