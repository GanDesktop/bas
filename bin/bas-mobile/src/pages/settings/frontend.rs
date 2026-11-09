use crate::shared::{
    i18n::tr,
    preferences::{self as prefs, Preferences},
    ui::{self, icon},
};
use nsh_base::{SelectItem, SliderValue, theme};
use nsh_core::prelude::*;

fn select(
    label: &str,
    options: Vec<(&str, String)>,
    read: fn(&Preferences) -> String,
    edit: fn(&mut Preferences, String),
) -> AnyElement {
    let items = options
        .iter()
        .map(|(v, label)| SelectItem::new(v.to_string(), label.clone()))
        .collect::<Vec<_>>();
    let labels = options
        .into_iter()
        .map(|(v, label)| (v.to_owned(), label))
        .collect::<Vec<_>>();
    let value = move || prefs::state().with(read);
    let display = move || {
        let current = value();
        labels
            .iter()
            .find(|(v, _)| *v == current)
            .map(|(_, label)| label.clone())
            .unwrap_or(current)
    };
    ui::setting(
        tr!(label),
        ui::picker(tr!(label))
            .items(items)
            .value(move || Some(value().into()))
            .on_change(move |v: &Option<SharedString>| {
                if let Some(v) = v {
                    prefs::change(|p| edit(p, v.to_string()));
                }
            })
            .trigger(
                view()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .min_h(px(44.0))
                    .child(text(display).text_size(crate::shared::ui::font_size))
                    .child(icon(ui::CHEVRON)),
            ),
    )
    .into_element()
}
fn toggle(
    key: &str,
    read: fn(&Preferences) -> bool,
    edit: fn(&mut Preferences, bool),
) -> AnyElement {
    ui::setting(
        tr!(key),
        ui::switch_control(
            tr!(key),
            move || prefs::state().with(read),
            move |v| prefs::change(|p| edit(p, *v)),
        ),
    )
    .into_element()
}
fn slider(
    key: &str,
    min: f32,
    max: f32,
    step: f32,
    read: fn(&Preferences) -> f32,
    edit: fn(&mut Preferences, f32),
) -> AnyElement {
    let label = tr!(key);
    view()
        .p_3()
        .gap_2()
        .child(
            view()
                .flex_row()
                .child(
                    text(label.clone())
                        .text_size(crate::shared::ui::font_size)
                        .flex_1(),
                )
                .child(
                    text(move || format!("{}", prefs::state().with(read)))
                        .text_size(crate::shared::ui::font_size),
                ),
        )
        .child(
            nsh_base::slider()
                .accessibility_label(label)
                .min(min as f64)
                .max(max as f64)
                .step(step as f64)
                .value(move || SliderValue::from(prefs::state().with(read) as f64))
                .on_change(move |v: &SliderValue| prefs::change(|p| edit(p, v.end() as f32)))
                .min_h(px(44.0))
                .track_style(|s| s.h(px(4.0)).rounded_full().bg(|| theme().muted_foreground))
                .range_style(|s| s.h_full().rounded_full().bg(|| theme().primary))
                .thumb_style(|s| s.size(px(20.0)).rounded_full().bg(|| theme().primary)),
        )
        .into_element()
}
pub(super) fn appearance() -> impl IntoElement {
    scroll().flex_1().child(
        view()
            .p_4()
            .child(ui::section(
                tr!("interface"),
                view()
                    .child(select(
                        "language",
                        vec![
                            ("system", tr!("system")),
                            ("zh-CN", "简体中文".into()),
                            ("en", "English".into()),
                        ],
                        |p| p.language.clone(),
                        |p, v| p.language = v,
                    ))
                    .child(toggle(
                        "animations",
                        |p| p.animations,
                        |p, v| p.animations = v,
                    ))
                    .child(select(
                        "color-scheme",
                        vec![
                            ("system", tr!("system")),
                            ("light", tr!("light")),
                            ("dark", tr!("dark")),
                        ],
                        |p| p.appearance.clone(),
                        |p, v| p.appearance = v,
                    )),
            ))
            .child(ui::section(
                tr!("fonts"),
                view()
                    .child(select(
                        "ui-font",
                        vec![
                            ("", tr!("system-font")),
                            ("sans-serif", tr!("sans-serif")),
                            ("serif", tr!("serif")),
                        ],
                        |p| p.ui_font.clone(),
                        |p, v| p.ui_font = v,
                    ))
                    .child(slider(
                        "ui-size",
                        12.0,
                        22.0,
                        1.0,
                        |p| p.ui_size,
                        |p, v| p.ui_size = v,
                    ))
                    .child(slider(
                        "content-size",
                        12.0,
                        24.0,
                        1.0,
                        |p| p.content_size,
                        |p, v| p.content_size = v,
                    ))
                    .child(select(
                        "code-font",
                        vec![("", tr!("system-font")), ("monospace", tr!("monospace"))],
                        |p| p.code_font.clone(),
                        |p, v| p.code_font = v,
                    )),
            ))
            .child(ui::section(
                tr!("typography"),
                slider(
                    "line-height",
                    1.2,
                    2.0,
                    0.1,
                    |p| p.line_height,
                    |p, v| p.line_height = v,
                ),
            ))
            .child(ui::section(
                tr!("background"),
                view()
                    .child(slider(
                        "main-opacity",
                        50.0,
                        100.0,
                        1.0,
                        |p| p.background_opacity,
                        |p, v| p.background_opacity = v,
                    ))
                    .child(slider(
                        "sidebar-opacity",
                        50.0,
                        100.0,
                        1.0,
                        |p| p.sidebar_opacity,
                        |p, v| p.sidebar_opacity = v,
                    )),
            ))
            .child(text(|| prefs::error().get()).text_color(|| theme().destructive)),
    )
}
pub(super) fn chat() -> impl IntoElement {
    scroll().flex_1().child(
        view().p_4().child(ui::section(
            tr!("chat-preferences"),
            view()
                .child(toggle(
                    "follow-messages",
                    |p| p.follow_messages,
                    |p, v| p.follow_messages = v,
                ))
                .child(toggle(
                    "show-thoughts",
                    |p| p.show_thoughts,
                    |p, v| p.show_thoughts = v,
                ))
                .child(toggle(
                    "expand-tools",
                    |p| p.expand_tools,
                    |p, v| p.expand_tools = v,
                )),
        )),
    )
}
pub(super) fn terminal() -> impl IntoElement {
    scroll().flex_1().child(
        view().p_4().child(ui::section(
            tr!("tool-output"),
            view()
                .child(slider(
                    "code-size",
                    11.0,
                    22.0,
                    1.0,
                    |p| p.code_size,
                    |p, v| p.code_size = v,
                ))
                .child(toggle(
                    "wrap-output",
                    |p| p.wrap_output,
                    |p, v| p.wrap_output = v,
                )),
        )),
    )
}
