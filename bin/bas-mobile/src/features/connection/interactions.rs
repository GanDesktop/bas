use super::{current, fail, state, task};
use crate::shared::api;
use bas_client::{acp, authentication::elicitation, model::PendingPermission, service::pb};
use nsh_core::prelude::*;
use std::{future::Future, time::Duration};

pub(crate) fn observe() {
    let s = state().get_untracked();
    let (Some(client), Some(session)) = (s.client.clone(), s.session.clone()) else {
        return;
    };
    let mut permissions = client.clone();
    let permission_session = session.clone();
    watch(
        async move {
            Ok(permissions
                .watch_permissions(pb::WatchPermissionsRequest {
                    session: permission_session,
                })
                .await?
                .into_inner())
        },
        |snapshot: pb::PermissionSnapshot| {
            let requests = snapshot
                .pending
                .into_iter()
                .map(|p| {
                    Ok(PendingPermission {
                        request_id: p.request_id,
                        request: p
                            .request
                            .ok_or_else(|| anyhow::anyhow!("Missing permission payload"))?
                            .decode()?,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>();
            match requests {
                Ok(pending) => state().update(|s| s.permissions = pending),
                Err(error) => fail(error.to_string()),
            }
        },
    );
    let mut elicitations = client;
    watch(
        async move {
            Ok(elicitations
                .watch_elicitations(pb::WatchElicitationsRequest { session })
                .await?
                .into_inner())
        },
        |snapshot: pb::ElicitationSnapshot| {
            state().update(|s| s.elicitations = snapshot.pending);
        },
    );
}
fn watch<T: std::fmt::Debug + Send + 'static>(
    start: impl Future<Output = anyhow::Result<tonic::Streaming<T>>> + Send + 'static,
    update: impl Fn(T) + 'static,
) {
    let s = state().get_untracked();
    task(async move {
        let mut stream = match background(start).await {
            Ok(Ok(stream)) => stream,
            other => {
                if current(s.generation, s.selection) {
                    fail(format!("{other:?}"));
                }
                return;
            }
        };
        while current(s.generation, s.selection) {
            let result = background(async move {
                let result = tokio::time::timeout(Duration::from_secs(1), stream.message()).await;
                (stream, result)
            })
            .await;
            if !current(s.generation, s.selection) {
                return;
            }
            match result {
                Ok((next, Ok(Ok(Some(value))))) => {
                    stream = next;
                    update(value);
                }
                Ok((next, Err(_))) => stream = next,
                Ok((_, Ok(Ok(None)))) => return,
                other => {
                    fail(format!("{other:?}"));
                    return;
                }
            }
        }
    });
}
pub(crate) fn permission(request: String, option: acp::PermissionOptionId) {
    let s = state().get_untracked();
    let Some(session) = s.session else {
        return;
    };
    let request_copy = request.clone();
    api::run(
        move |mut client| async move {
            client
                .respond_permission_acp(
                    session.parse()?,
                    request,
                    acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                        option,
                    )),
                )
                .await?;
            Ok(())
        },
        move |result| {
            if !current(s.generation, s.selection) {
                return;
            }
            match result {
                Ok(()) => {
                    state().update(|s| s.permissions.retain(|p| p.request_id != request_copy))
                }
                Err(error) => fail(error.to_string()),
            }
        },
    );
}
pub(crate) fn answer(pending: pb::PendingElicitation, response: serde_json::Value) {
    let s = state().get_untracked();
    let Some(session) = s.session else {
        return;
    };
    let validation = (|| {
        let raw = pending
            .request
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing elicitation payload"))?
            .decode()?;
        let request = elicitation::parse_request(&raw)?;
        elicitation::validate_response(&request, &response)?;
        anyhow::Ok(())
    })();
    if let Err(error) = validation {
        fail(error.to_string());
        return;
    }
    api::run(
        move |mut client| async move {
            client
                .respond_elicitation(pb::RespondElicitationRequest {
                    session,
                    request_id: pending.request_id,
                    response: Some(pb::AcpPayload {
                        json: serde_json::to_vec(&response)?,
                    }),
                })
                .await?;
            Ok(())
        },
        move |result| {
            if current(s.generation, s.selection)
                && let Err(error) = result
            {
                fail(error.to_string());
            }
        },
    );
}
