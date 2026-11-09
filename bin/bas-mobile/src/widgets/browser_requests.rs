use crate::{
    features::{
        connection,
        local_service::{self, browser},
    },
    shared::{i18n::tr, routes, ui},
};
use gds_uim::el;
use nsh_base::{SheetEvent, sheet, theme};
use nsh_core::prelude::*;

fn in_login() -> bool {
    routes::page().get() == routes::Page::Authentication
        && connection::state().with(|s| s.endpoint == local_service::status().get().endpoint)
}

pub(crate) fn inline() -> impl IntoElement {
    when(in_login, panel)
}

pub(crate) fn overlay() -> impl IntoElement {
    sheet()
        .side(Placement::Bottom)
        .open(|| Some(!in_login() && browser::state().with(|s| !s.pending.is_empty())))
        .on_open_change(|event: &SheetEvent| {
            if matches!(event, SheetEvent::OpenChange(false, _)) {
                if let Some(request) = browser::state().get_untracked().pending.first().cloned() {
                    browser::dismiss(request);
                }
            }
        })
        .child(
            scroll().max_h(window().height * 0.75).child(
                view()
                    .p_4()
                    .pb_safe_area_plus(px(16.0))
                    .mb_keyboard()
                    .bg(|| theme().background)
                    .child(panel()),
            ),
        )
}

fn panel() -> impl IntoElement {
    el! { .view { gap_3 } [
        $(when(|| browser::state().with(|s| !s.pending.is_empty()), || view().gap_2()
            .child(text(tr!("browser-request-title")).font_weight(FontWeight::SEMIBOLD))
            .child(text(tr!("browser-request-help")).text_size(ui::font_size).text_color(|| theme().muted_foreground)))),
        $(each(|| browser::state().with(|s| s.pending.clone()), |r| r.id, |request, _| {
            let request = request.get();
            let open = request.clone();
            let host = bas_browser_bridge::web_url(&request.url).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_default();
            el! { .view { gap_2; p_3; rounded_lg; border_1; border_color: $move || theme().border; bg: $move || theme().card } [
                $(text(host)) { font_weight: FontWeight::SEMIBOLD; text_size: ui::font_size },
                .view { flex_row; flex_wrap; gap_2; justify_end } [
                    $(ui::labeled_action(ui::BACK, tr!("browser-dismiss"), move || browser::dismiss(request.clone())).disabled(|| browser::state().with(|s| s.busy))),
                    $(ui::labeled_action(ui::EXTERNAL, tr!("browser-open"), move || browser::open(open.clone())).disabled(|| browser::state().with(|s| s.busy))),
                ],
            ] }
        })),
        $(when(|| browser::state().with(|s| !s.pending.is_empty() && !s.error.is_empty()), || text(|| browser::state().with(|s| s.error.clone())).text_size(ui::font_size).text_color(|| theme().destructive))),
    ] }
}
