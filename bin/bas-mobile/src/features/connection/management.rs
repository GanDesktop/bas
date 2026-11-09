use super::{current, fail, state, task};
use crate::shared::{api, i18n::tr};
use bas_client::service::pb;

pub(crate) fn load_icons() {
    api::run(
        |mut client| async move {
            let catalog = client
                .list_agent_catalog(pb::ListAgentCatalogRequest { refresh: false })
                .await?
                .into_inner();
            Ok(serde_json::from_slice::<bas_client::management::Catalog>(
                &catalog.catalog_json,
            )?)
        },
        |result| {
            if let Ok(catalog) = result {
                state().update(|s| {
                    s.agent_icons = catalog
                        .entries
                        .into_iter()
                        .filter(|entry| !entry.icon.is_empty())
                        .map(|entry| (entry.id, entry.icon))
                        .collect()
                });
            }
        },
    );
}

pub(crate) fn reload() {
    load_icons();
    api::run(
        |mut client| async move {
            let agents = client
                .list_agents(pb::ListAgentsRequest {})
                .await?
                .into_inner()
                .agents;
            let workspaces = client
                .list_workspaces(pb::ListWorkspacesRequest::default())
                .await?
                .into_inner()
                .workspaces;
            let sessions = client
                .list_sessions(pb::ListSessionsRequest {
                    include_archived: true,
                })
                .await?
                .into_inner()
                .sessions;
            Ok((agents, workspaces, sessions))
        },
        |result| match result {
            Ok((agents, workspaces, sessions)) => state().update(|s| {
                s.agents = agents;
                s.workspaces = workspaces;
                s.sessions = sessions;
                if !s.agents.iter().any(|a| Some(&a.id) == s.agent.as_ref()) {
                    s.agent = s.agents.first().map(|a| a.id.clone());
                }
                if !s
                    .agents
                    .iter()
                    .any(|a| Some(&a.id) == s.draft_agent.as_ref())
                {
                    s.draft_agent = s.agents.first().map(|a| a.id.clone());
                }
            }),
            Err(error) => fail(error.to_string()),
        },
    );
}
pub(crate) fn rename() {
    let s = state().get_untracked();
    let Some(target) = s.rename_session else {
        return;
    };
    let session = target.id;
    let title = s.title_draft.trim().to_owned();
    api::run(
        move |mut client| async move {
            client
                .set_title(pb::SetTitleRequest { session, title })
                .await?;
            Ok(())
        },
        move |result| {
            if !current(s.generation, s.selection) {
                return;
            }
            match result {
                Ok(()) => {
                    state().update(|s| s.rename_session = None);
                    reload();
                }
                Err(e) => fail(e.to_string()),
            }
        },
    );
}
#[derive(Clone, Copy)]
pub(crate) enum Action {
    Close,
    Restart,
    Archive,
    Restore,
    Delete,
}
pub(crate) fn perform(action: Action, session: String) {
    let s = state().get_untracked();
    task(async move {
        let title = match action {
            Action::Close => tr!("close-session"),
            Action::Restart => tr!("restart-session"),
            Action::Archive => tr!("archive-session"),
            Action::Restore => tr!("restore-session"),
            Action::Delete => tr!("delete-session"),
        };
        let choice = nsh_base::alert_dialog()
            .title(title.clone())
            .cancel("cancel", tr!("dismiss"))
            .action("confirm", title)
            .show()
            .await;
        if !current(s.generation, s.selection)
            || !choice.is_ok_and(|c| c.action.as_deref() == Some("confirm"))
        {
            return;
        }
        let target = session.clone();
        api::run(
            move |mut client| async move {
                match action {
                    Action::Close => {
                        client
                            .close_session(pb::GetSessionRequest { session })
                            .await?;
                    }
                    Action::Restart => {
                        client
                            .restart_session(pb::GetSessionRequest { session })
                            .await?;
                    }
                    Action::Archive | Action::Restore => {
                        client
                            .set_archived(pb::SetArchivedRequest {
                                session,
                                archived: matches!(action, Action::Archive),
                            })
                            .await?;
                    }
                    Action::Delete => {
                        client
                            .delete_session(pb::DeleteSessionRequest { session })
                            .await?;
                    }
                }
                Ok(())
            },
            move |result| {
                if !current(s.generation, s.selection) {
                    return;
                }
                match result {
                    Ok(()) => {
                        state().update(|s| {
                            s.rename_session = None;
                            if matches!(action, Action::Archive | Action::Delete)
                                && s.session.as_deref() == Some(&target)
                            {
                                s.selection += 1;
                                s.session = None;
                                s.messages.clear();
                                s.permissions.clear();
                                s.elicitations.clear();
                            }
                        });
                        reload();
                    }
                    Err(error) => fail(error.to_string()),
                }
            },
        );
    });
}
