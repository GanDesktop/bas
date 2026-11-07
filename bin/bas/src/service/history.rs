//! History pagination, dependency-aligned windows, and navigation.
use super::*;
use super::{pagination::PageEncoder, status::metadata_error};
use bas_protocol::model::Cursor;

/// History page size used when a client does not ask for one.
const DEFAULT_PAGE: usize = 128;

impl Service {
    pub(super) async fn handle_load_history(
        &self,
        request: Request<pb::LoadHistoryRequest>,
    ) -> Result<Response<pb::LoadHistoryResponse>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let limit = if request.limit == 0 {
            DEFAULT_PAGE
        } else {
            request.limit as usize
        };
        let mut encoder = PageEncoder::new(bas_protocol::protocol::MAX_MESSAGE_SIZE);
        let mut result = Ok(true);
        session
            .try_visit_history(
                request.after.map(Cursor::from_raw),
                limit,
                |event, has_more| {
                    result = encoder.push(pb::Event::from(event), has_more);
                    matches!(result, Ok(true))
                },
            )
            .await?;
        result?;
        Ok(Response::new(encoder.finish()))
    }

    pub(super) async fn handle_load_history_window(
        &self,
        request: Request<pb::LoadHistoryWindowRequest>,
    ) -> Result<Response<pb::LoadHistoryWindowResponse>, Status> {
        use prost::Message as _;

        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let window = session
            .history_window(request.before.map(Cursor::from_raw), request.limit as usize)
            .await?;
        let response = encode_history_window(&window)?;
        if response.encoded_len() > bas_protocol::protocol::MAX_MESSAGE_SIZE {
            return Err(Status::resource_exhausted(
                "dependency-aligned history window exceeds the RPC payload limit",
            ));
        }
        Ok(Response::new(response))
    }

    pub(super) async fn handle_load_history_overview(
        &self,
        request: Request<pb::LoadHistoryOverviewRequest>,
    ) -> Result<Response<pb::LoadHistoryOverviewResponse>, Status> {
        use prost::Message as _;

        let session = self.session(&request.into_inner().session).await?;
        let overview = session.history_overview().await?;
        let response = pb::LoadHistoryOverviewResponse {
            session: overview.session.to_string(),
            cursor: overview.cursor,
            entries: overview
                .entries
                .into_iter()
                .map(|entry| pb::HistoryOverviewEntry {
                    cursor: entry.cursor,
                    timestamp_millis: entry.timestamp_millis,
                    preview: entry.preview,
                })
                .collect(),
        };
        if response.encoded_len() > bas_protocol::protocol::MAX_MESSAGE_SIZE {
            return Err(Status::resource_exhausted(
                "complete history overview exceeds the RPC payload limit",
            ));
        }
        Ok(Response::new(response))
    }

    pub(super) async fn handle_load_history_window_at(
        &self,
        request: Request<pb::LoadHistoryWindowAtRequest>,
    ) -> Result<Response<pb::LoadHistoryWindowAtResponse>, Status> {
        use prost::Message as _;

        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let location = session
            .history_window_at(Cursor::from_raw(request.cursor))
            .await?;
        let response = pb::LoadHistoryWindowAtResponse {
            window: Some(encode_history_window(&location.window)?),
            after: location.after,
        };
        if response.encoded_len() > bas_protocol::protocol::MAX_MESSAGE_SIZE {
            return Err(Status::resource_exhausted(
                "dependency-aligned history location exceeds the RPC payload limit",
            ));
        }
        Ok(Response::new(response))
    }
}

fn encode_history_window(
    window: &bas_protocol::protocol::HistoryWindow,
) -> Result<pb::LoadHistoryWindowResponse, Status> {
    Ok(pb::LoadHistoryWindowResponse {
        session: Some(pb::Session::try_from(&window.session).map_err(metadata_error)?),
        events: window.events.iter().map(pb::Event::from).collect(),
        context: window.context.iter().map(pb::Event::from).collect(),
        before: window.before,
        cursor: window.cursor,
    })
}
