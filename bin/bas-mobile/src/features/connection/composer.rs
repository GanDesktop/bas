use super::{current, fail, state, task};
use crate::shared::{api, i18n::tr};
use bas_client::{acp, service::pb};
use base64::{Engine, engine::general_purpose::STANDARD};
use nsh_core::prelude::*;

#[derive(Clone, PartialEq)]
pub(crate) struct Attachment {
    pub id: u64,
    pub name: String,
    pub content: acp::ContentBlock,
}

pub(crate) fn session() -> Option<pb::Session> {
    state().with(|s| {
        s.sessions
            .iter()
            .find(|row| Some(&row.id) == s.session.as_ref())
            .cloned()
    })
}
pub(crate) fn configuration() -> Option<acp::NewSessionResponse> {
    session()?.acp?.decode().ok()
}
pub(crate) fn options() -> Vec<acp::SessionConfigOption> {
    configuration()
        .and_then(|s| s.config_options)
        .unwrap_or_default()
}
pub(crate) fn configure(id: acp::SessionConfigId, value: acp::SessionConfigOptionValue) {
    let s = state().get_untracked();
    let Some(session) = s.session.clone() else {
        return;
    };
    if s.configuring {
        return;
    }
    state().update(|s| {
        s.configuring = true;
        s.error.clear();
    });
    api::run(
        move |mut client| async move {
            client
                .set_config_option_acp(&acp::SetSessionConfigOptionRequest::new(
                    session.clone(),
                    id,
                    value,
                ))
                .await?;
            Ok(client
                .get_session(pb::GetSessionRequest { session })
                .await?
                .into_inner())
        },
        move |result| {
            if !current(s.generation, s.selection) {
                return;
            }
            state().update(|s| s.configuring = false);
            match result {
                Ok(response) => update(response),
                Err(error) => fail(error.to_string()),
            }
        },
    );
}
pub(crate) fn set_mode(value: &Option<SharedString>) {
    let Some(value) = value else {
        return;
    };
    let mode = value.to_string();
    let s = state().get_untracked();
    let Some(session) = s.session.clone() else {
        return;
    };
    if s.configuring {
        return;
    }
    state().update(|s| s.configuring = true);
    api::run(
        move |mut client| async move {
            client
                .set_mode_acp(&acp::SetSessionModeRequest::new(session.clone(), mode))
                .await?;
            Ok(client
                .get_session(pb::GetSessionRequest { session })
                .await?
                .into_inner())
        },
        move |result| {
            if !current(s.generation, s.selection) {
                return;
            }
            state().update(|s| s.configuring = false);
            match result {
                Ok(response) => update(response),
                Err(error) => fail(error.to_string()),
            }
        },
    );
}
fn update(response: pb::GetSessionResponse) {
    if let Some(session) = response.session {
        state().update(|s| {
            if let Some(row) = s.sessions.iter_mut().find(|row| row.id == session.id) {
                *row = session;
            }
        });
    }
}
pub(crate) fn attach() {
    pick(false);
}
pub(crate) fn attach_images() {
    pick(true);
}
fn pick(images: bool) {
    let s = state().get_untracked();
    let caps = session()
        .and_then(|s| s.initialization)
        .and_then(|p| p.decode::<acp::InitializeResponse>().ok())
        .map(|i| i.agent_capabilities.prompt_capabilities)
        .unwrap_or_default();
    task(async move {
        let result = if images {
            pick_images(nsh_core::picker::PickRequest::new(true, 10)).await
        } else {
            pick_files(nsh_core::picker::FilePickRequest::new(true, 10)).await
        };
        if !current(s.generation, s.selection) {
            return;
        }
        for file in result {
            let data = STANDARD.encode(&file.data);
            let content = if file.mime.starts_with("image/") && caps.image {
                acp::ContentBlock::Image(acp::ImageContent::new(data, file.mime))
            } else if file.mime.starts_with("audio/") && caps.audio {
                acp::ContentBlock::Audio(acp::AudioContent::new(data, file.mime))
            } else if caps.embedded_context {
                let uri = format!("attachment:///{}", file.name);
                let resource = match std::str::from_utf8(&file.data) {
                    Ok(text) => acp::EmbeddedResourceResource::TextResourceContents(
                        acp::TextResourceContents::new(text, uri).mime_type(file.mime),
                    ),
                    Err(_) => acp::EmbeddedResourceResource::BlobResourceContents(
                        acp::BlobResourceContents::new(data, uri).mime_type(file.mime),
                    ),
                };
                acp::ContentBlock::Resource(acp::EmbeddedResource::new(resource))
            } else {
                fail(tr!("unsupported-attachment"));
                continue;
            };
            state().update(|s| {
                static NEXT_ATTACHMENT: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(1);
                s.attachments.push(Attachment {
                    id: NEXT_ATTACHMENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    name: file.name,
                    content,
                })
            });
        }
    });
}
