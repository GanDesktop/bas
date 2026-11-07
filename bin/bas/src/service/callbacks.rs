//! Permission and elicitation snapshots and answers.
use super::*;
use bas_protocol::acp;

impl Service {
    pub(super) async fn handle_watch_permissions(
        &self,
        request: Request<pb::WatchPermissionsRequest>,
    ) -> Result<Response<EventStream<pb::PermissionSnapshot>>, Status> {
        let session = self.session(&request.into_inner().session).await?;
        let id = session.id();
        let subscription = self.registry.permissions().subscribe(id);
        // Deletion can race subscription setup after the first registry lookup.
        if let Err(error) = self.registry.get(id).await {
            self.registry.permissions().close(id);
            return Err(Status::not_found(error.to_string()));
        }
        let registry = Arc::clone(&self.registry);
        let stream =
            futures::stream::try_unfold((subscription, true), move |(mut subscription, first)| {
                let registry = Arc::clone(&registry);
                async move {
                    if !first {
                        tokio::select! {
                            biased;
                            () = registry.stopped() => return Ok(None),
                            result = subscription.snapshots.changed() => {
                                if result.is_err() { return Ok(None); }
                            }
                        }
                    }
                    let pending = subscription
                        .snapshots
                        .borrow_and_update()
                        .iter()
                        .map(pb::PendingPermission::from)
                        .collect();
                    Ok(Some((
                        pb::PermissionSnapshot {
                            session: id.to_string(),
                            pending,
                        },
                        (subscription, false),
                    )))
                }
            });
        Ok(Response::new(Box::pin(stream)))
    }

    pub(super) async fn handle_respond_permission(
        &self,
        request: Request<pb::RespondPermissionRequest>,
    ) -> Result<Response<pb::RespondPermissionResponse>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let response: acp::RequestPermissionResponse = request
            .response
            .ok_or_else(|| Status::invalid_argument("missing ACP permission response"))?
            .decode()
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        self.registry
            .permissions()
            .respond(session.id(), &request.request_id, response.outcome)
            .map_err(|error| match error {
                crate::agent::permissions::PermissionError::InvalidOption => {
                    Status::invalid_argument(error.to_string())
                }
                crate::agent::permissions::PermissionError::Expired => {
                    Status::failed_precondition(error.to_string())
                }
            })?;
        Ok(Response::new(pb::RespondPermissionResponse {}))
    }

    pub(super) async fn handle_watch_elicitations(
        &self,
        request: Request<pb::WatchElicitationsRequest>,
    ) -> Result<Response<EventStream<pb::ElicitationSnapshot>>, Status> {
        let session = self.session(&request.into_inner().session).await?;
        let id = session.id();
        let subscription = self.registry.elicitations().subscribe(id);
        if let Err(error) = self.registry.get(id).await {
            self.registry.elicitations().close(id);
            return Err(Status::not_found(error.to_string()));
        }
        let registry = Arc::clone(&self.registry);
        let stream = futures::stream::try_unfold(
            (subscription, true),
            move |(mut subscription, first)| {
                let registry = Arc::clone(&registry);
                async move {
                    if !first {
                        tokio::select! {
                            biased;
                            () = registry.stopped() => return Ok(None),
                            result = subscription.snapshots.changed() => if result.is_err() { return Ok(None); },
                        }
                    }
                    let snapshot = subscription.snapshots.borrow_and_update().clone();
                    Ok(Some((
                        pb::ElicitationSnapshot {
                            session: id.to_string(),
                            pending: snapshot
                                .pending
                                .iter()
                                .map(|pending| pb::PendingElicitation {
                                    request_id: pending.request_id.clone(),
                                    request: Some((&pending.request).into()),
                                    answered: pending.answered,
                                })
                                .collect(),
                            completed_ids: snapshot.completed_ids,
                        },
                        (subscription, false),
                    )))
                }
            },
        );
        Ok(Response::new(Box::pin(stream)))
    }

    pub(super) async fn handle_respond_elicitation(
        &self,
        request: Request<pb::RespondElicitationRequest>,
    ) -> Result<Response<pb::RespondElicitationResponse>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let response = request
            .response
            .ok_or_else(|| Status::invalid_argument("Missing elicitation response"))?
            .decode()
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        self.registry
            .elicitations()
            .respond(session.id(), &request.request_id, response)
            .map_err(|error| match error {
                crate::agent::elicitation::ElicitationError::Invalid(_) => {
                    Status::invalid_argument(error.to_string())
                }
                crate::agent::elicitation::ElicitationError::Expired => {
                    Status::failed_precondition(error.to_string())
                }
            })?;
        Ok(Response::new(pb::RespondElicitationResponse {}))
    }
}
