use crate::{
    features::{agent_management::auth, connection},
    shared::{
        i18n::tr,
        routes,
        ui::{self, ContentPage},
    },
};
use bas_client::{authentication::Execution, service::pb};
use gds_uim::el;
use nsh_base::theme;
use nsh_core::prelude::*;

pub(crate) fn render() -> impl IntoElement {
    let header = el! { .view { px_2; py_1; flex_row; items_center; gap_2; border_b_1; border_color: $move || theme().border } [
        $(ui::icon_action(ui::BACK, tr!("back"), routes::back)),
        $(text(tr!("agent-sign-in"))) { flex_1; font_weight: FontWeight::SEMIBOLD },
    ] };
    let content = scroll().flex_1().child(el! { .view { test_id: "authentication-page"; p_4; gap_4 } [
        .view { gap_2 } [
            $(ui::icon(ui::SHIELD)),
            $(text(|| auth::state().with(|s| s.entry.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| s.agent.clone())))) { text_lg; font_weight: FontWeight::SEMIBOLD },
            $(text(|| connection::profiles::name(&connection::state().get_untracked().endpoint))) { text_sm; text_color: $move || theme().muted_foreground },
            $(text(tr!("auth-page-help"))) { text_size: ui::font_size; text_color: $move || theme().muted_foreground },
        ],
        $(when(|| auth::state().with(|s| s.loading), || text(tr!("auth-loading")))),
        $(when(|| auth::state().with(|s| !s.message.is_empty()), || text(|| auth::state().with(|s| s.message.clone())).text_size(ui::font_size))),
        $(when(|| auth::state().with(|s| !s.error.is_empty()), || text(|| auth::state().with(|s| s.error.clone())).text_size(ui::font_size).text_color(|| theme().destructive))),
        $(when(|| auth::state().with(|s| !s.loading && !s.running) && !auth::connected(), methods)),
        $(each(|| auth::state().with(|s| s.pending.clone()), |p| p.request_id.clone(), |pending, _| {
            dynamic(move || form(pending.get()))
        })),
    ] });
    let footer = el! { .view { p_4; gap_2; border_t_1; border_color: $move || theme().border } [
        $(when(|| auth::state().with(|s| s.running), || ui::labeled_action(ui::STOP, tr!("auth-cancel"), auth::stop))),
        $(when(|| auth::state().with(|s| !s.loading && !s.running) && !auth::connected(), || ui::labeled_action(ui::REFRESH, tr!("auth-recheck"), auth::reload))),
        $(when(auth::connected, || ui::labeled_action(ui::BACK, tr!("auth-done"), routes::back))),
    ] };
    ContentPage::default()
        .header(Some(header.into_element()))
        .content(Some(content.into_element()))
        .footer(Some(footer.into_element()))
        .render()
}

fn methods() -> impl IntoElement {
    el! { .view { gap_3 } [
        $(when(|| auth::state().with(|s| s.methods.is_empty()), || text(tr!("auth-no-methods")).text_size(ui::font_size))),
        $(each(|| auth::state().with(|s| s.methods.clone()), |m| m.id.clone(), |method, _| {
            let method = method.get();
            let reason = match &method.execution {
                Execution::Agent => None,
                Execution::Terminal(_) => Some(tr!("terminal-auth-required")),
                Execution::Unsupported { reason } => Some(reason.clone()),
            };
            let disabled = reason.is_some();
            let label = method.name.clone();
            let description = method.description.clone();
            el! { .view { p_3; gap_2; rounded_lg; border_1; border_color: $move || theme().border; bg: $move || theme().card } [
                $(ui::labeled_action(ui::LOGIN, label, move || auth::start(method.clone())).disabled(disabled)),
                $(description.map(|value| text(value).text_size(ui::font_size))),
                $(reason.map(|value| text(value).text_size(ui::font_size).text_color(|| theme().muted_foreground))),
            ] }
        })),
    ] }
}

fn form(pending: pb::PendingElicitation) -> AnyElement {
    if pending.answered {
        return text(tr!("awaiting-auth"))
            .text_size(ui::font_size)
            .into_element();
    }
    match auth::request(&pending) {
        Ok(request) => view()
            .p_3()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(|| theme().border)
            .child(ui::elicitation::render(request, move |value| {
                auth::answer(pending.clone(), value)
            }))
            .into_element(),
        Err(error) => text(error.to_string())
            .text_size(ui::font_size)
            .text_color(|| theme().destructive)
            .into_element(),
    }
}
