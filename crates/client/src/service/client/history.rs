//! Validated raw-event history loaders shared by all frontends.

use std::time::{Duration, Instant};

use prost::Message as _;
use tonic::Status;

use super::Client;
use crate::{model::SessionId, service::pb};

use bas_protocol::protocol::{HistoryLocation, HistoryOverview, HistoryWindow};

impl Client {
    /// Loads every prompt marker without loading transcript bodies.
    ///
    /// # Errors
    /// Returns transport/deadline errors, `ResourceExhausted` if the complete
    /// overview cannot fit, or `DataLoss` for incomplete or malformed history.
    pub async fn history_overview(
        &mut self,
        session: SessionId,
        timeout: Duration,
    ) -> Result<HistoryOverview, Status> {
        let response = tokio::time::timeout(
            timeout,
            self.load_history_overview(pb::LoadHistoryOverviewRequest {
                session: session.to_string(),
            }),
        )
        .await
        .map_err(|_| deadline())??
        .into_inner();
        HistoryOverview::decode(session, response)
    }

    /// Loads the complete logical page containing `cursor` directly.
    /// Dependencies may expand it backwards, never through unrelated newer pages.
    ///
    /// # Errors
    /// Returns `InvalidArgument` for zero/future cursors, storage/transport errors,
    /// `ResourceExhausted` for an oversized page, or `DataLoss` for invalid replies.
    pub async fn history_window_at(
        &mut self,
        session: SessionId,
        cursor: u64,
        timeout: Duration,
    ) -> Result<HistoryLocation, Status> {
        if cursor == 0 {
            return Err(Status::invalid_argument(
                "history location cursor must be positive",
            ));
        }
        let response = tokio::time::timeout(
            timeout,
            self.load_history_window_at(pb::LoadHistoryWindowAtRequest {
                session: session.to_string(),
                cursor,
            }),
        )
        .await
        .map_err(|_| deadline())??
        .into_inner();
        HistoryLocation::decode(session, cursor, response)
    }

    /// Loads `limit` complete user turns, newest first in chronological order.
    /// Initialization is a separate segment. Replay dependencies can expand the
    /// range by whole turns; a long turn is never silently split.
    ///
    /// # Errors
    /// Returns `InvalidArgument` for zero limit or a non-boundary `before`,
    /// `ResourceExhausted` for a safe range too large for one RPC,
    /// `DataLoss` for expired dependencies or an
    /// invalid response, and transport/deadline errors without retrying.
    pub async fn history_window(
        &mut self,
        session: SessionId,
        before: Option<u64>,
        limit: u32,
        timeout: Duration,
    ) -> Result<HistoryWindow, Status> {
        if limit == 0 {
            return Err(Status::invalid_argument(
                "history window limit must be positive",
            ));
        }
        let started = Instant::now();
        let response = tokio::time::timeout(
            timeout,
            self.load_history_window(pb::LoadHistoryWindowRequest {
                session: session.to_string(),
                before,
                limit,
            }),
        )
        .await
        .map_err(|_| deadline())??
        .into_inner();
        let rpc_ms = started.elapsed().as_millis();
        let protobuf_bytes = response.encoded_len();
        let decoding = Instant::now();
        let window = HistoryWindow::decode(session, before, response)?;
        tracing::debug!(
            %session, ?before, requested_turns = limit,
            events = window.events.len(), protobuf_bytes, rpc_ms,
            decode_ms = decoding.elapsed().as_millis(),
            "Loaded history window"
        );
        Ok(window)
    }
}

fn deadline() -> Status {
    Status::deadline_exceeded("history RPC timed out")
}

#[cfg(test)]
mod tests;
