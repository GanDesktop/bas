//! Separate management and ACP observation streams.
use super::*;
use crate::session::AttachOptions;
use bas_protocol::model::Cursor;
use futures::StreamExt;

impl Service {
    pub(super) async fn handle_attach(
        &self,
        request: Request<pb::AttachRequest>,
    ) -> Result<Response<EventStream<pb::ManagementEvent>>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let attachment = session
            .try_attach(AttachOptions {
                replay: request.replay as usize,
                cursor: request.cursor.map(Cursor::from_raw),
            })
            .await?;
        let stream = attachment.into_stream().filter_map(|event| {
            futures::future::ready(match event {
                Ok(event) => pb::ManagementEvent::from_event(&event).map(Ok),
                Err(error) => Some(Err(error.into())),
            })
        });
        Ok(Response::new(Box::pin(stream)))
    }

    pub(super) async fn handle_observe_acp(
        &self,
        request: Request<pb::AttachRequest>,
    ) -> Result<Response<EventStream<pb::AcpObservation>>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let attachment = session
            .try_attach(AttachOptions {
                replay: request.replay as usize,
                cursor: request.cursor.map(Cursor::from_raw),
            })
            .await?;
        let stream = attachment.into_stream().filter_map(|event| {
            futures::future::ready(match event {
                Ok(event) => pb::AcpObservation::from_event(&event).map(Ok),
                Err(error) => Some(Err(error.into())),
            })
        });
        Ok(Response::new(Box::pin(stream)))
    }
}
