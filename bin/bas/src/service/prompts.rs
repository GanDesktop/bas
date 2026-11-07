//! Prompt admission, acceptance lookup, and explicit cancellation.
use super::decode;
use super::*;
use crate::session::{CreateError, SubmissionError};
use bas_protocol::{
    acp,
    model::{
        PromptRejection, PromptRejectionKind, PromptSubmission, PromptSubmissionOutcome,
        TurnReceipt,
    },
};

impl Service {
    pub(super) async fn handle_prompt(
        &self,
        request: Request<pb::PromptSubmission>,
    ) -> Result<Response<pb::PromptSubmissionResponse>, Status> {
        let submission = match PromptSubmission::try_from(request.into_inner()) {
            Ok(submission) => submission,
            Err(error) => {
                return submission_response(Err(PromptRejection {
                    kind: PromptRejectionKind::InvalidRequest,
                    message: error.to_string(),
                }
                .into()));
            }
        };
        let session = match self
            .session(&submission.request.session_id.to_string())
            .await
        {
            Ok(session) => session,
            Err(error) => {
                return submission_response(Err(PromptRejection {
                    kind: if error.code() == tonic::Code::InvalidArgument {
                        PromptRejectionKind::InvalidRequest
                    } else {
                        PromptRejectionKind::Unavailable
                    },
                    message: error.message().to_owned(),
                }
                .into()));
            }
        };
        match session.known_submission(&submission).await {
            Ok(Some(receipt)) => return submission_response(Ok(receipt)),
            Ok(None) => {}
            Err(error) => return submission_response(Err(error)),
        }
        if session.meta().archived {
            return submission_response(session.accept_prompt(submission).await);
        }
        let _backend = match self.registry.use_backend(&session).await {
            Ok(lease) => lease,
            Err(error) => {
                // Another caller may have registered this identity while restoration
                // was pending. An accepted submission remains queryable when inactive.
                return match session.known_submission(&submission).await {
                    Ok(Some(receipt)) => submission_response(Ok(receipt)),
                    Err(error) => submission_response(Err(error)),
                    Ok(None)
                        if matches!(error, CreateError::Storage(_) | CreateError::Session(_)) =>
                    {
                        Err(error.into())
                    }
                    Ok(None) => submission_response(Err(PromptRejection {
                        kind: PromptRejectionKind::Unavailable,
                        message: Status::from(error).message().to_owned(),
                    }
                    .into())),
                };
            }
        };
        let result = session.accept_prompt(submission).await;
        if result.is_ok() {
            self.registry.record_agent_use(session.meta().agent);
        }
        submission_response(result)
    }

    pub(super) async fn handle_get_prompt_acceptance(
        &self,
        request: Request<pb::PromptAcceptanceRequest>,
    ) -> Result<Response<pb::PromptAcceptanceResponse>, Status> {
        let request = request.into_inner();
        let turn = request
            .turn
            .parse()
            .map_err(|error: bas_protocol::model::ParseIdError| {
                Status::invalid_argument(error.to_string())
            })?;
        let session = self.session(&request.session).await?;
        let accepted = session.prompt_acceptance(turn).await?;
        Ok(Response::new(pb::PromptAcceptanceResponse {
            accepted: accepted.map(Into::into),
        }))
    }

    pub(super) async fn handle_cancel(
        &self,
        request: Request<pb::AcpPayload>,
    ) -> Result<Response<pb::CancelResponse>, Status> {
        let request: acp::CancelNotification = decode(request)?;
        let session = self.session(&request.session_id.to_string()).await?;
        if session.meta().status == bas_protocol::model::SessionStatus::Stopped {
            return Ok(Response::new(pb::CancelResponse {}));
        }
        let _backend = self.registry.use_backend(&session).await?;
        session.cancel_acp(request).await?;
        self.registry.permissions().cancel(session.id());
        self.registry.elicitations().cancel(session.id());

        Ok(Response::new(pb::CancelResponse {}))
    }
}

fn submission_response(
    result: Result<TurnReceipt, SubmissionError>,
) -> Result<Response<pb::PromptSubmissionResponse>, Status> {
    let outcome = match result {
        Ok(receipt) => PromptSubmissionOutcome::Accepted(receipt),
        Err(SubmissionError::Rejected(rejection)) => PromptSubmissionOutcome::Rejected(rejection),
        Err(SubmissionError::Unknown(error)) => return Err(error.into()),
    };
    Ok(Response::new(outcome.into()))
}
