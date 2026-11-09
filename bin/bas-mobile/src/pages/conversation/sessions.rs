use super::workspace_name;
use crate::{
    features::connection::{self as model, navigation as nav, state},
    shared::{
        i18n::tr,
        preferences,
        ui::{self, ContentPage, ControlRow, icon, icon_action, picker},
    },
    widgets::backend,
};
use gds_uim::{el, m};
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;
fn visible_sessions() -> Vec<bas_client::service::pb::Session> {
    let query = state().with(|s| s.session_search.to_lowercase());
    let filter = nav::filter().get();
    let mut sessions = model::sessions()
        .into_iter()
        .filter(|s| {
            let matching = s
                .title
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&query);
            matching
                && match filter {
                    nav::Filter::Recent => !s.archived && !nav::pinned(&s.id),
                    nav::Filter::Bookmarked => {
                        !s.archived && nav::bookmarked(&s.id) && !nav::pinned(&s.id)
                    }
                    nav::Filter::Archived => s.archived,
                }
        })
        .collect::<Vec<_>>();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    sessions
}
fn pinned_sessions() -> Vec<bas_client::service::pb::Session> {
    let mut sessions = state().with(|s| {
        s.sessions
            .iter()
            .filter(|s| !s.archived && nav::pinned(&s.id))
            .cloned()
            .collect::<Vec<_>>()
    });
    sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    sessions
}
#[derive(Clone, PartialEq)]
enum Row {
    Period(String),
    Session(Box<bas_client::service::pb::Session>),
}
fn rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let mut last = String::new();
    for session in visible_sessions() {
        let period = crate::shared::time::period(session.updated_at);
        if period != last {
            rows.push(Row::Period(period.clone()));
            last = period;
        }
        rows.push(Row::Session(Box::new(session)));
    }
    rows
}
fn filters() -> impl IntoElement {
    let open = signal(false);
    ui::popup(open)
        .trigger(ui::trigger(ui::LIST_FILTER, tr!("filter-sessions")))
        .child(
            view()
                .child(filter_action(
                    open,
                    nav::Filter::Recent,
                    ui::CLOCK,
                    "sessions-by-time",
                ))
                .child(filter_action(
                    open,
                    nav::Filter::Bookmarked,
                    ui::BOOKMARK,
                    "bookmarked-sessions",
                ))
                .child(filter_action(
                    open,
                    nav::Filter::Archived,
                    ui::ARCHIVE,
                    "archived-sessions",
                )),
        )
}
fn filter_action(
    open: Signal<bool>,
    filter: nav::Filter,
    icon: ImageAsset,
    label: &str,
) -> impl IntoElement {
    ui::labeled_action(icon, tr!(label), move || {
        nav::filter().set(filter);
        open.set(false);
    })
    .selected(move || nav::filter().get() == filter)
    .selected_style(m! { bg: $move || theme().accent })
}
pub(super) fn sidebar() -> impl IntoElement {
    let search = signal(false);
    let header = el! { .view { p_3; gap_2 } [
        $(backend::picker()),
        $(when(|| !pinned_sessions().is_empty(), || view().pb_2().border_b_1().border_color(||theme().border)
            .child(text(tr!("pinned-sessions")).px_3().py_2().text_xs().text_color(||theme().muted_foreground))
            .child(scroll().max_h(px(192.0)).child(each(pinned_sessions, |s|s.id.clone(), |s,_|session_row(move ||Some(s.get()),px(0.0))))))),
        $(picker(tr!("workspace")).items(|| state().with(|s| s.workspaces.iter().map(|w| SelectItem::new(w.id.clone(),w.name.clone())).collect::<Vec<_>>()))
            .value(|| state().with(|s| s.workspace.clone().map(Into::into))).on_change(model::choose_workspace)
            .trigger(ControlRow::default().leading(Some(icon(ui::FOLDER).into_element())).content(Some(text(workspace_name).text_size(crate::shared::ui::font_size).truncate().into_element())).trailing(Some(icon(ui::CHEVRON).into_element())).render())),
        $(super::new_session::NewSession::default().align(Align::Start).render()),
        $(when(|| visible_sessions().is_empty(), || text(tr!("no-sessions")).p_3().text_size(ui::font_size).text_color(||theme().muted_foreground))),
        .view { flex_row; items_center; pl_3; pt_2 } [
            $(text(||tr!(match nav::filter().get() { nav::Filter::Recent => "sessions", nav::Filter::Bookmarked => "bookmarked-sessions", nav::Filter::Archived => "archived-sessions" }))) { text_size: crate::shared::ui::font_size; flex_1; text_color: $move || theme().muted_foreground },
            $(filters()),
            $(icon_action(ui::SEARCH,tr!("search-sessions"),move ||search.update(|v| *v = !*v))),
        ],
        $(when(search, || ui::field(tr!("search-sessions"), ||state().with(|s|s.session_search.clone()), |v|state().update(|s|s.session_search=v.into())))),
    ] };
    let entries = computed(rows);
    let content = virtual_list(
        move || entries.with(Vec::len),
        move |at| {
            let row = move || entries.with(|rows| rows.get(at.index()).cloned());
            view()
                .child(when(
                    move || matches!(row(), Some(Row::Period(_))),
                    move || {
                        text(move || match row() {
                            Some(Row::Period(label)) => label,
                            _ => String::new(),
                        })
                        .px_6()
                        .pt_3()
                        .pb_2()
                        .text_xs()
                        .text_color(|| theme().muted_foreground)
                    },
                ))
                .child(when(
                    move || matches!(row(), Some(Row::Session(_))),
                    move || {
                        session_row(
                            move || match row() {
                                Some(Row::Session(s)) => Some(*s),
                                _ => None,
                            },
                            px(12.0),
                        )
                    },
                ))
        },
    )
    .keys(move || {
        entries.with(|rows| {
            rows.iter()
                .map(|row| match row {
                    Row::Period(label) => format!("period:{label}"),
                    Row::Session(s) => s.id.clone(),
                })
                .collect::<Vec<_>>()
        })
    })
    .flex_1();
    view()
        .w_full()
        .flex_1()
        .py_safe_area()
        .mb_keyboard()
        .bg(|| {
            theme()
                .sidebar
                .opacity(preferences::state().with(|p| p.sidebar_opacity / 100.0))
        })
        .text_color(|| theme().sidebar_foreground)
        .child(
            ContentPage::default()
                .header(Some(header.into_element()))
                .content(Some(content.into_element()))
                .render(),
        )
}

fn session_row(
    row: impl Fn() -> Option<bas_client::service::pb::Session> + Copy + 'static,
    inset: Points,
) -> impl IntoElement {
    let open = signal(false);
    ui::context_menu(open)
        .trigger(
            view()
                .role(AccessibilityRole::Button)
                .on_long_press(move || open.set(true))
                .on_press(move || {
                    if let Some(session) = row() {
                        model::select_session(session.id);
                    }
                })
                .label(move || {
                    row()
                        .and_then(|s| s.title)
                        .unwrap_or_else(|| tr!("unknown"))
                })
                .mx(inset)
                .mb_1()
                .rounded_lg()
                .bg(move || {
                    if row().is_some_and(|r| state().with(|s| s.session.as_ref() == Some(&r.id))) {
                        theme().sidebar_accent
                    } else {
                        AdaptiveColor::TRANSPARENT
                    }
                })
                .child(
                    ControlRow::default()
                        .leading(Some(
                            crate::widgets::agent::icon(move || {
                                row().map(|s| s.agent).unwrap_or_default()
                            })
                            .into_element(),
                        ))
                        .content(Some(
                            text(move || {
                                row()
                                    .and_then(|s| s.title)
                                    .unwrap_or_else(|| tr!("unknown"))
                            })
                            .text_size(crate::shared::ui::font_size)
                            .truncate()
                            .into_element(),
                        ))
                        .trailing(Some(
                            when(
                                move || row().is_some_and(|s| nav::bookmarked(&s.id)),
                                || icon(ui::BOOKMARK),
                            )
                            .into_element(),
                        ))
                        .render(),
                ),
        )
        .child(
            view()
                .gap_1()
                .child(dynamic(move || {
                    let pinned = row().is_some_and(|s| nav::pinned(&s.id));
                    ui::menu_action(
                        ui::PIN,
                        tr!(if pinned {
                            "unpin-session"
                        } else {
                            "pin-session"
                        }),
                        move || {
                            open.set(false);
                            if let Some(s) = row() {
                                nav::toggle_pin(&s.id);
                            }
                        },
                    )
                }))
                .child(dynamic(move || {
                    let marked = row().is_some_and(|s| nav::bookmarked(&s.id));
                    ui::menu_action(
                        ui::BOOKMARK,
                        tr!(if marked {
                            "remove-bookmark"
                        } else {
                            "add-bookmark"
                        }),
                        move || {
                            open.set(false);
                            if let Some(s) = row() {
                                nav::toggle_bookmark(&s.id);
                            }
                        },
                    )
                }))
                .child(ui::menu_action(ui::COMPOSE, tr!("rename"), move || {
                    open.set(false);
                    if let Some(session) = row() {
                        state().update(|s| {
                            s.title_draft = session.title.clone().unwrap_or_default();
                            s.rename_session = Some(session);
                        });
                    }
                }))
                .child(ui::menu_action(ui::REFRESH, tr!("refresh"), move || {
                    open.set(false);
                    if let Some(session) = row() {
                        if state().with(|s| s.session.as_ref() == Some(&session.id)) {
                            model::refresh();
                        } else {
                            model::select_session(session.id);
                        }
                    }
                }))
                .child(ui::menu_action(ui::STOP, tr!("close-session"), move || {
                    open.set(false);
                    if let Some(session) = row() {
                        model::management::perform(model::management::Action::Close, session.id);
                    }
                }))
                .child(ui::menu_action(
                    ui::REFRESH,
                    tr!("restart-session"),
                    move || {
                        open.set(false);
                        if let Some(session) = row() {
                            model::management::perform(
                                model::management::Action::Restart,
                                session.id,
                            );
                        }
                    },
                ))
                .child(ui::menu_action(
                    if row().is_some_and(|s| s.archived) {
                        ui::ARCHIVE_RESTORE
                    } else {
                        ui::ARCHIVE
                    },
                    tr!(if row().is_some_and(|s| s.archived) {
                        "restore-session"
                    } else {
                        "archive-session"
                    }),
                    move || {
                        open.set(false);
                        if let Some(session) = row() {
                            model::management::perform(
                                if session.archived {
                                    model::management::Action::Restore
                                } else {
                                    model::management::Action::Archive
                                },
                                session.id,
                            );
                        }
                    },
                ))
                .child(
                    ui::menu_action(ui::TRASH, tr!("delete-session"), move || {
                        open.set(false);
                        if let Some(session) = row() {
                            model::management::perform(
                                model::management::Action::Delete,
                                session.id,
                            );
                        }
                    })
                    .text_color(|| theme().destructive),
                ),
        )
}
