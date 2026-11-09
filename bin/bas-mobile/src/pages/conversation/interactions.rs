use crate::{
    features::connection::{interactions as model, state},
    shared::{i18n::tr, ui},
};
use bas_client::{authentication::elicitation, service::pb};
use nsh_base::theme;
use nsh_core::prelude::*;

pub(super) fn render() -> impl IntoElement {
    view()
        .gap_2()
        .child(each(
            || state().with(|s| s.permissions.clone()),
            |p| p.request_id.clone(),
            |pending, _| {
                let pending = pending.get();
                let mut actions = view().gap_1();
                for option in pending.request.options {
                    let request = pending.request_id.clone();
                    actions =
                        actions.child(ui::labeled_action(ui::SHIELD, option.name, move || {
                            model::permission(request.clone(), option.option_id.clone())
                        }));
                }
                view()
                    .p_3()
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(|| theme().border)
                    .bg(|| theme().card)
                    .child(
                        text(tr!("permission"))
                            .text_size(crate::shared::ui::font_size)
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        text(pending.request.tool_call.fields.title.unwrap_or_default())
                            .text_size(crate::shared::ui::font_size),
                    )
                    .child(actions)
            },
        ))
        .child(each(
            || state().with(|s| s.elicitations.clone()),
            |p| p.request_id.clone(),
            |pending, _| dynamic(move || render_elicitation(pending.get())),
        ))
}
fn render_elicitation(pending: pb::PendingElicitation) -> AnyElement {
    if pending.answered {
        return text(tr!("awaiting-auth"))
            .p_3()
            .text_size(crate::shared::ui::font_size)
            .into_element();
    }
    let parsed = pending
        .request
        .as_ref()
        .and_then(|p| p.decode::<serde_json::Value>().ok())
        .ok_or_else(|| "Missing elicitation payload".to_owned())
        .and_then(|raw| elicitation::parse_request(&raw).map_err(|e| e.to_string()));
    match parsed {
        Ok(request) => view()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(|| theme().border)
            .child(ui::elicitation::render(request, move |response| {
                model::answer(pending.clone(), response)
            }))
            .into_element(),
        Err(error) => text(error)
            .p_3()
            .text_size(crate::shared::ui::font_size)
            .text_color(|| theme().destructive)
            .into_element(),
    }
}
