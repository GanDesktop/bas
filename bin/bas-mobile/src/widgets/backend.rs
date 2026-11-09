use crate::{
    features::connection::{self as model, state},
    shared::{
        i18n::tr,
        ui::{self, ControlRow, icon},
    },
};
use nsh_base::{SelectEntry, SelectItem, theme};
use nsh_core::prelude::*;

pub(crate) fn name() -> String {
    state().with(|s| {
        if s.endpoint.is_empty() {
            tr!("backend")
        } else {
            model::profiles::name(&s.endpoint)
        }
    })
}
pub(crate) fn picker() -> impl IntoElement {
    ui::picker(tr!("backend"))
        .items(|| {
            state().with(|s| {
                s.addresses
                    .iter()
                    .map(|a| SelectItem::new(a.clone(), model::profiles::name(a)))
                    .collect::<Vec<_>>()
            })
        })
        .render_item(|entry: &SelectEntry| {
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(icon(connection_icon(&entry.value)))
                .child(
                    text(entry.title.clone())
                        .text_size(ui::font_size)
                        .truncate(),
                )
        })
        .value(|| state().with(|s| Some(s.endpoint.clone().into())))
        .on_change(model::choose_backend)
        .trigger(
            ControlRow::default()
                .leading(Some(
                    image(|| state().with(|s| connection_icon(&s.endpoint)))
                        .size(px(18.0))
                        .tint_color(|| theme().foreground)
                        .into_element(),
                ))
                .content(Some(
                    view()
                        .gap_1()
                        .child(
                            text(name)
                                .text_size(crate::shared::ui::font_size)
                                .truncate(),
                        )
                        .child(
                            text(|| {
                                state().with(|s| {
                                    if s.loading {
                                        tr!("loading")
                                    } else if s.connected {
                                        tr!("ready")
                                    } else {
                                        tr!("offline")
                                    }
                                })
                            })
                            .text_xs()
                            .text_color(|| theme().muted_foreground),
                        )
                        .into_element(),
                ))
                .trailing(Some(icon(ui::CHEVRON).into_element()))
                .render(),
        )
}

pub(crate) fn connection_icon(endpoint: &str) -> ImageAsset {
    if endpoint == "local" || endpoint.starts_with("unix:") || endpoint.starts_with('/') {
        ui::MONITOR
    } else {
        ui::SERVER
    }
}
