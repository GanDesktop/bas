use crate::{
    features::session_policy as policy,
    shared::{
        i18n::tr,
        ui::{self, field, icon_action, picker},
    },
    widgets::backend,
};
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;

pub(super) fn render() -> impl IntoElement {
    policy::load();
    scroll().flex_1().child(
        view()
            .p_4()
            .gap_3()
            .child(backend::picker())
            .child(ui::section(
                tr!("idle-timeout"),
                ui::setting(
                    tr!("idle-seconds"),
                    field(
                        tr!("idle-seconds"),
                        || policy::state().with(|s| s.idle.clone()),
                        policy::edit_idle,
                    )
                    .w(px(100.0))
                    .keyboard_type(KeyboardType::Number),
                ),
            ))
            .child(ui::section(
                tr!("prewarm"),
                view()
                    .child(ui::setting(
                        tr!("default-agent"),
                        picker(tr!("default-agent"))
                            .items(|| {
                                let mut items = vec![SelectItem::new("@last", tr!("last-agent"))];
                                items.extend(
                                    policy::agent_ids()
                                        .into_iter()
                                        .map(|id| SelectItem::new(id.clone(), id)),
                                );
                                items
                            })
                            .value(|| {
                                Some(
                                    policy::state()
                                        .with(|s| {
                                            s.policy
                                                .default_agent
                                                .as_ref()
                                                .map(ToString::to_string)
                                                .unwrap_or_else(|| "@last".into())
                                        })
                                        .into(),
                                )
                            })
                            .on_change(policy::choose_default)
                            .trigger(
                                view().min_h(px(44.0)).justify_center().child(
                                    text(|| {
                                        policy::state().with(|s| {
                                            s.policy
                                                .default_agent
                                                .as_ref()
                                                .map(ToString::to_string)
                                                .unwrap_or_else(|| tr!("last-agent"))
                                        })
                                    })
                                    .text_size(crate::shared::ui::font_size),
                                ),
                            ),
                    ))
                    .child(ui::setting(
                        tr!("pool-size"),
                        field(
                            tr!("pool-size"),
                            || policy::state().with(|s| s.size.clone()),
                            policy::edit_size,
                        )
                        .w(px(72.0))
                        .keyboard_type(KeyboardType::Number),
                    )),
            ))
            .child(ui::section(
                tr!("pool-overrides"),
                view()
                    .child(each(
                        || {
                            policy::state().with(|s| {
                                s.overrides
                                    .iter()
                                    .map(|(id, _)| id.clone())
                                    .collect::<Vec<_>>()
                            })
                        },
                        |id| id.clone(),
                        |id, _| {
                            let id = id.get();
                            let read = id.clone();
                            let edit = id.clone();
                            let remove = id.clone();
                            ui::setting(
                                id.clone(),
                                view()
                                    .flex_row()
                                    .items_center()
                                    .child(
                                        field(
                                            id,
                                            move || {
                                                policy::state().with(|s| {
                                                    s.overrides
                                                        .iter()
                                                        .find(|(id, _)| *id == read)
                                                        .map(|(_, v)| v.clone())
                                                        .unwrap_or_default()
                                                })
                                            },
                                            move |v| policy::edit_override(&edit, v),
                                        )
                                        .w(px(64.0))
                                        .keyboard_type(KeyboardType::Number),
                                    )
                                    .child(icon_action(
                                        ui::TRASH,
                                        tr!("remove-override"),
                                        move || policy::remove_override(&remove),
                                    )),
                            )
                        },
                    ))
                    .child(
                        picker(tr!("add-override"))
                            .items(|| {
                                policy::agent_ids()
                                    .into_iter()
                                    .map(|id| SelectItem::new(id.clone(), id))
                                    .collect::<Vec<_>>()
                            })
                            .on_change(policy::add_override)
                            .trigger(ui::labeled_action(ui::PLUS, tr!("add-override"), || {})),
                    ),
            ))
            .child(
                text(tr!("override-zero"))
                    .text_xs()
                    .text_color(|| theme().muted_foreground),
            )
            .child(
                text(|| policy::state().with(|s| s.error.clone()))
                    .text_size(crate::shared::ui::font_size)
                    .text_color(|| theme().destructive),
            )
            .child(
                ui::labeled_action(ui::REFRESH, tr!("reload"), policy::load)
                    .disabled(|| policy::state().with(|s| s.busy)),
            ),
    )
}
