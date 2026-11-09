use crate::{
    features::agent_management as agents,
    shared::{
        i18n::tr,
        routes,
        ui::{self, ContentPage, field, icon, icon_action, labeled_action},
    },
    widgets::backend,
};
use bas_client::management::{Entry, Source, Status};
use gds_uim::el;
use nsh_base::theme;
use nsh_core::prelude::*;
fn feedback() -> impl IntoElement {
    view()
        .px_4()
        .gap_2()
        .child(when(
            || agents::state().with(|s| !s.error.is_empty()),
            || {
                text(|| agents::state().with(|s| s.error.clone()))
                    .text_size(crate::shared::ui::font_size)
                    .text_color(|| theme().destructive)
            },
        ))
        .child(when(
            || agents::state().with(|s| s.busy),
            || {
                text(|| {
                    agents::state().with(|s| {
                        if s.message.is_empty() {
                            tr!("loading")
                        } else {
                            s.message.clone()
                        }
                    })
                })
                .text_size(crate::shared::ui::font_size)
                .text_color(|| theme().muted_foreground)
            },
        ))
        .child(when(
            || {
                agents::state().with(|s| {
                    s.catalog
                        .as_ref()
                        .and_then(|c| c.warning.as_ref())
                        .is_some()
                })
            },
            || {
                text(|| {
                    agents::state().with(|s| {
                        s.catalog
                            .as_ref()
                            .and_then(|c| c.warning.clone())
                            .unwrap_or_default()
                    })
                })
                .text_size(crate::shared::ui::font_size)
            },
        ))
}
pub(super) fn list(catalog: bool) -> impl IntoElement {
    agents::load(false);
    let header = el! { .view { p_3; gap_2 } [
        $(backend::picker()),
        .view { flex_row; items_center; justify_between } [
            $(labeled_action(ui::PLUS, if catalog { tr!("custom-agent") } else { tr!("add-agent") }, move || if catalog { agents::new_agent(); } else { routes::catalog(); }).disabled(|| agents::state().with(|s| s.busy || s.configuration.is_none()))),
            $(icon_action(ui::REFRESH, tr!("refresh"), agents::refresh).disabled(|| agents::state().with(|s| s.busy))),
        ],
        $(field(tr!("search-agents"), || agents::state().with(|s| s.search.clone()), agents::search)),
        $(feedback()),
    ] };
    ContentPage::default()
        .header(Some(header.into_element()))
        .content(Some(
            virtual_list(
                move || agents::entries(catalog).len(),
                move |at| {
                    dynamic(move || {
                        let entries = agents::entries(catalog);
                        entries.get(at.index()).cloned().map(|entry| {
                            let first = at.index() == 0
                                || entries
                                    .get(at.index().saturating_sub(1))
                                    .is_some_and(|prev| prev.source != entry.source);
                            let title = match entry.source {
                                Source::Registry => tr!("registry"),
                                Source::Custom => tr!("custom-agents"),
                                Source::Embedded => tr!("builtin-agents"),
                            };
                            view()
                                .child(if first && !catalog {
                                    text(title)
                                        .px_4()
                                        .pt_4()
                                        .pb_2()
                                        .text_size(ui::font_size)
                                        .text_color(|| theme().muted_foreground)
                                        .into_element()
                                } else {
                                    view().into_element()
                                })
                                .child(agent_card(entry))
                        })
                    })
                },
            )
            .flex_1()
            .into_element(),
        ))
        .render()
}
fn agent_card(entry: Entry) -> impl IntoElement {
    let installed = entry.spec.is_some();
    let open = entry.clone();
    let chat = entry.id.clone();
    let install = entry.clone();
    el! { .view { test_id: $(&format!("agent-{}",entry.id)); mx_3; py_2; flex_row; items_center; border_b_1; border_color: $move ||theme().border } [
        $(ui::action(move ||agents::details(open.clone())).flex_1().child(view().flex_row().items_center().gap_3().px_2()
            .child(if entry.icon.is_empty(){icon(ui::BOT).into_element()}else{image(entry.icon.clone()).size(px(24.0)).tint_color(||theme().primary).into_element()})
            .child(view().flex_1().gap_1().child(text(entry.name).font_weight(FontWeight::SEMIBOLD).text_size(crate::shared::ui::font_size)).child(text(if entry.version.is_empty(){entry.id.clone()}else{format!("{} · {}",entry.id,entry.version)}).text_xs().text_color(||theme().muted_foreground))))),
        $(if installed { icon_action(ui::CHAT,tr!("start-chat"),move ||routes::chat(chat.clone())).into_element() } else {
            icon_action(ui::DOWNLOAD,tr!("install-agent"),move ||agents::install(install.clone())).disabled(||agents::state().with(|s|s.busy||!s.catalog.as_ref().is_some_and(|c|c.can_install))).into_element()
        }),
    ] }
}
pub(super) fn detail() -> impl IntoElement {
    scroll().flex_1().child(
        view()
            .p_4()
            .gap_3()
            .child(backend::picker())
            .child(dynamic(|| {
                agents::state()
                    .with(|s| s.selected.clone())
                    .map(detail_content)
            }))
            .child(feedback())
            .child(when(
                || agents::state().with(|s| s.check.is_some()),
                check_result,
            )),
    )
}
fn detail_content(entry: Entry) -> impl IntoElement {
    let installed = entry.spec.is_some();
    let custom = entry.source == Source::Custom;
    let acp = entry.source != Source::Embedded;
    let update = entry.source == Source::Registry
        && (entry.status == Status::NotInstalled
            || entry.status == Status::Missing
            || entry
                .installed_version
                .as_ref()
                .is_some_and(|v| v != &entry.version));
    let chat = entry.id.clone();
    let check = entry.clone();
    let login = entry.clone();
    let install = entry.clone();
    let edit = entry.clone();
    let logout = entry.clone();
    let remove = entry.clone();
    let repository = entry.repository;
    let license = entry.license_url;
    let mut content = view()
        .gap_2()
        .child(text(entry.name).text_lg().font_weight(FontWeight::SEMIBOLD))
        .child(
            text(entry.version)
                .text_size(crate::shared::ui::font_size)
                .text_color(|| theme().muted_foreground),
        )
        .child(text(entry.description).text_size(crate::shared::ui::font_size));
    if installed {
        content = content.child(
            labeled_action(ui::CHAT, tr!("start-chat"), move || {
                routes::chat(chat.clone())
            })
            .bg(|| theme().primary)
            .text_color(|| theme().primary_foreground),
        );
    }
    if installed && acp {
        content = content.child(labeled_action(ui::LOGIN, tr!("authenticate"), move || {
            agents::auth::open(login.clone(), None);
        }));
        content = content.child(
            labeled_action(ui::ACTIVITY, tr!("check-agent"), move || {
                agents::check(check.clone())
            })
            .disabled(|| agents::state().with(|s| s.busy)),
        );
    }
    if update {
        content = content.child(
            labeled_action(ui::DOWNLOAD, tr!("install-agent"), move || {
                agents::install(install.clone())
            })
            .disabled(|| agents::state().with(|s| s.busy)),
        );
    }
    if custom {
        content = content.child(labeled_action(ui::SETTINGS, tr!("edit-agent"), move || {
            agents::edit(edit.clone());
        }));
    }
    if !repository.is_empty() {
        content = content.child(labeled_action(ui::EXTERNAL, tr!("repository"), move || {
            let url = repository.clone();
            spawn(async move {
                open_url(url).await;
            });
        }));
    }
    if !license.is_empty() {
        content = content.child(labeled_action(ui::SHIELD, tr!("license"), move || {
            let url = license.clone();
            spawn(async move {
                open_url(url).await;
            });
        }));
    }
    if installed && acp {
        content = content
            .child(
                labeled_action(ui::LOGOUT, tr!("logout"), move || {
                    confirm_logout(logout.clone())
                })
                .text_color(|| theme().destructive),
            )
            .child(
                labeled_action(ui::TRASH, tr!("remove-agent"), move || {
                    confirm_remove(remove.clone())
                })
                .text_color(|| theme().destructive),
            );
    }
    content
}
fn confirm_remove(entry: Entry) {
    spawn(async move {
        let result = nsh_base::alert_dialog()
            .title(tr!("remove-agent"))
            .description(entry.name.clone())
            .cancel("cancel", tr!("dismiss"))
            .destructive("remove", tr!("remove-agent"))
            .show()
            .await;
        if result.is_ok_and(|r| r.action.as_deref() == Some("remove")) {
            agents::state().update(|s| s.pending_remove = Some(entry));
            agents::confirm_remove();
        }
    });
}
fn confirm_logout(entry: Entry) {
    spawn(async move {
        let result = nsh_base::alert_dialog()
            .title(tr!("logout"))
            .description(entry.name.clone())
            .cancel("cancel", tr!("dismiss"))
            .action("logout", tr!("logout"))
            .show()
            .await;
        if result.is_ok_and(|r| r.action.as_deref() == Some("logout")) {
            agents::logout(entry);
        }
    });
}
fn check_result() -> impl IntoElement {
    el! { .view { p_4; gap_2; border_t_1; border_color: $move || theme().border } [
        $(text(|| agents::state().with(|s| s.check.as_ref().map(|(entry, _)| entry.name.clone()).unwrap_or_default()))) { font_weight: FontWeight::SEMIBOLD },
        $(text(|| agents::state().with(|s| s.check.as_ref().map(|(_, check)| if check.message.is_empty() { format!("{:?}", check.status()) } else { check.message.clone() }).unwrap_or_default()))) { text_sm },
        $(when(|| agents::state().with(|s| s.check.as_ref().is_some_and(|(_, c)| c.status() == bas_client::service::pb::AgentCheckStatus::AuthenticationRequired)), || {
            labeled_action(ui::LOGIN, tr!("authenticate"), || {
                if let Some((entry, check)) = agents::state().get_untracked().check { agents::auth::open(entry, Some(check)); }
            })
        })),
        $(labeled_action(ui::BACK, tr!("close"), || agents::state().update(|s| s.check = None))),
    ] }
}
pub(super) fn editor() -> impl IntoElement {
    scroll().flex_1().child(el! { .view { p_4; gap_3 } [
        tr!("agent-id").text { text_size: crate::shared::ui::font_size; text_color: $move ||theme().muted_foreground },
        $(field(tr!("agent-id"), || agents::state().with(|s| s.editor.id.clone()), |v| agents::state().update(|s| s.editor.id = v.into())).disabled(|| agents::state().with(|s| s.busy || s.editor.original.is_some()))),
        tr!("description").text { text_size: crate::shared::ui::font_size; text_color: $move ||theme().muted_foreground },
        $(field(tr!("description"), || agents::state().with(|s| s.editor.description.clone()), |v| agents::state().update(|s| s.editor.description = v.into()))),
        tr!("program").text { text_size: crate::shared::ui::font_size; text_color: $move ||theme().muted_foreground },
        $(field(tr!("program"), || agents::state().with(|s| s.editor.program.clone()), |v| agents::state().update(|s| s.editor.program = v.into()))),
        tr!("arguments").text { text_size: crate::shared::ui::font_size; text_color: $move ||theme().muted_foreground },
        $(field(tr!("arguments"), || agents::state().with(|s| s.editor.args.clone()), |v| agents::state().update(|s| s.editor.args = v.into())).multiline(true).h(px(96.0))),
        tr!("environment").text { text_size: crate::shared::ui::font_size; text_color: $move ||theme().muted_foreground },
        $(field(tr!("environment"), || agents::state().with(|s| s.editor.env.clone()), |v| agents::state().update(|s| s.editor.env = v.into())).multiline(true).h(px(96.0))),
        tr!("cancellation-grace").text { text_size: crate::shared::ui::font_size; text_color: $move ||theme().muted_foreground },
        $(field(tr!("cancellation-grace"), || agents::state().with(|s| s.editor.grace.clone()), |v| agents::state().update(|s| s.editor.grace = v.into())).keyboard_type(KeyboardType::Number)),
        $(feedback()),

    ] })
}
