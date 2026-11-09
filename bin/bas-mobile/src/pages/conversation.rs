mod attachments;
mod composer;
mod interactions;
mod message;
mod navigation;
mod new_session;
mod sessions;
mod timeline;

use crate::{
    features::connection::{self as model, state},
    shared::{
        i18n::tr,
        preferences,
        routes::{self, Page},
        ui::{self, ContentPage, icon_action},
    },
};
use gds_uim::{el, m};
use nsh_base::{SheetEvent, sheet, theme};
use nsh_core::prelude::*;

const DRAWER_WIDTH: f32 = 320.0;
pub(crate) fn render() -> impl IntoElement {
    el! { .view { flex_1; pt_safe_area; px_safe_area; mb_keyboard; bg: $move || theme().background.opacity(preferences::state().with(|p|p.background_opacity/100.0)); text_color: $move || theme().foreground } [
        $(when(|| state().with(|s| s.connected), connected)),
        $(when(|| state().with(|s| !s.connected), welcome)),
    ] }
}
fn welcome() -> impl IntoElement {
    ContentPage::default()
        .header(Some(
            view()
                .flex_row()
                .justify_end()
                .p_2()
                .child(app_menu())
                .into_element(),
        ))
        .content(Some(
            view()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    ui::labeled_action(ui::PLUS, tr!("new-connection"), || {
                        routes::open(Page::Connections)
                    })
                    .bg(|| theme().accent),
                )
                .into_element(),
        ))
        .render()
}
fn connected() -> impl IntoElement {
    let navigation = navigation::Navigation::new();
    provide_context(navigation);
    let page = ContentPage::default()
        .header(Some(header().into_element()))
        .content(Some(transcript().into_element()))
        .footer(Some(composer::render().into_element()));
    el! { .view { flex_1 } [
        $(page.render()),
        $(navigation::panel(navigation)),
        .sheet { side: Placement::Left; popup_style: m! { w: DRAWER_WIDTH.px; max_w: 0.9.nsh_core::geometry::Percentage }; open: $move || Some(state().with(|s| s.drawer)); on_open_change: drawer_changed; backdrop_class: "bg-black/30" } [$(sessions::sidebar())],
        .sheet { side: Placement::Bottom; open: $move || Some(state().with(|s| s.rename_session.is_some())); on_open_change: $|e: &SheetEvent| { if matches!(e, SheetEvent::OpenChange(false, _)) { state().update(|s| s.rename_session=None); } } } [
            .view { w_full; p_4; pb_safe_area_plus: 16.0.px; mb_keyboard; gap_3; bg: $move || theme().background } [
                tr!("rename").text { font_weight: FontWeight::SEMIBOLD },
                $(ui::field(tr!("title"), || state().with(|s| s.title_draft.clone()), |v| state().update(|s| s.title_draft=v.into()))),
                $(ui::labeled_action(ui::SAVE, tr!("save"), model::management::rename)),
            ],
        ],
    ] }
}
fn drawer_changed(event: &SheetEvent) {
    if let SheetEvent::OpenChange(open, _) = event {
        state().update(|s| s.drawer = *open);
    }
}
fn header() -> impl IntoElement {
    el! { .view { flex_row; items_center; gap_2; px_2; py_2 } [
        $(icon_action(ui::MENU, tr!("menu"), model::open_drawer)),
        .view { flex_1; min_w: 0.0.px; gap_1 } [
            $(text(|| state().with(|s| s.sessions.iter().find(|row| Some(&row.id)==s.session.as_ref()).and_then(|s| s.title.clone()).unwrap_or_else(|| "BAS".into())))) { text_size: crate::shared::ui::font_size; truncate; font_weight: FontWeight::SEMIBOLD },
            $(text(workspace_name)) { text_xs; truncate; text_color: $move || theme().muted_foreground },
        ],
        $(when(|| state().with(|s| s.session.is_some()), || new_session::NewSession::default()
            .align(Align::End)
            .trigger(Some(ui::trigger(ui::COMPOSE, tr!("new-session")).into_element()))
            .render())),
        $(app_menu()),
    ] }
}
fn app_menu() -> impl IntoElement {
    let open = signal(false);
    ui::popup(open)
        .trigger(ui::trigger(ui::MORE, tr!("app-menu")))
        .child(
            view()
                .gap_1()
                .child(ui::labeled_action(
                    ui::SETTINGS,
                    tr!("settings"),
                    move || {
                        open.set(false);
                        model::close_drawer();
                        routes::open(Page::Settings);
                    },
                ))
                .child(ui::labeled_action(ui::INFO, tr!("about"), move || {
                    open.set(false);
                    model::close_drawer();
                    routes::open(Page::About);
                })),
        )
}
fn workspace_name() -> String {
    state().with(|s| {
        s.workspaces
            .iter()
            .find(|w| Some(&w.id) == s.workspace.as_ref())
            .map(|w| w.name.clone())
            .unwrap_or_else(|| tr!("workspace"))
    })
}
fn transcript() -> impl IntoElement {
    view()
        .flex_1()
        .child(when(
            || state().with(|s| s.messages.is_empty()),
            || {
                view()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(
                        image(ui::BOT)
                            .size(px(36.0))
                            .tint_color(move || theme().muted_foreground),
                    )
                    .child(text("BAS").text_lg().font_weight(FontWeight::SEMIBOLD))
            },
        ))
        .child(when(
            || state().with(|s| !s.messages.is_empty()),
            timeline::render,
        ))
}
