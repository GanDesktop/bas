use crate::{
    features::connection::{self as model, Execution, composer as data, state},
    shared::{
        i18n::tr,
        preferences,
        ui::{self, icon, picker},
    },
};
use bas_client::acp;
use gds_uim::el;
use nsh_base::{SelectItem, input, theme};
use nsh_core::prelude::*;

pub(super) fn render() -> impl IntoElement {
    let focused = signal(false);
    let harness = signal(false);
    let attachments = signal(false);
    let model_open = signal(false);
    let editor = element_ref();
    let keyboard_seen = stored(false);
    effect(move || {
        let visible = keyboard_height() > Points::ZERO;
        if keyboard_seen.replace(visible)
            && !visible
            && !harness.get_untracked()
            && !attachments.get_untracked()
            && !model_open.get_untracked()
        {
            editor.blur();
            focused.set(false);
        }
    });
    let expanded = move || {
        focused.get() || keyboard_height() > Points::ZERO || harness.get() || model_open.get()
    };
    let editor_padding = move || {
        if expanded() {
            px(8.0)
        } else {
            px((44.0 - preferences::state().with(|p| p.content_size) * 1.5).max(0.0) / 2.0)
        }
    };
    // Keep the native input type stable: changing it reconnects Android's IME.
    el! { .view { px_3; pt_2; pb: $move || px(8.0)+if keyboard_height()>Points::ZERO {Points::ZERO}else{safe_area().bottom} } [
        $(when(||state().with(|s|!s.error.is_empty()),||text(||state().with(|s|s.error.clone())).text_size(crate::shared::ui::font_size).text_color(||theme().destructive).mb_2())),
        $(when(||state().with(|s|s.execution.submitting()||s.execution.running()),||text(||state().with(|s|match s.execution {
            Execution::Submitting => tr!("prompt-submitting"),
            Execution::Running => tr!("prompt-running"),
            _ => String::new(),
        })).test_id("prompt-progress").text_size(ui::font_size).text_color(||theme().muted_foreground).mb_2())),
        $(when(||state().with(|s|!s.permissions.is_empty()||!s.elicitations.is_empty()), ||view().mb_2().child(super::interactions::render()))),
        $(when(||state().with(|s|s.login_agent().is_some()), || view().mb_2().gap_2()
            .child(text(tr!("auth-required")).text_size(ui::font_size))
            .child(ui::labeled_action(ui::LOGIN, tr!("authenticate"), || {
                if let Some(agent) = state().get_untracked().login_agent() {
                    crate::features::agent_management::auth::open_agent(agent);
                }
            })))),
        $(super::attachments::render()),
        .view { test_id: "composer"; grid; grid_template_columns: "44px minmax(0, 1fr) 44px"; grid_template_rows: $move || SharedString::from(if expanded(){"76px 44px"}else{"44px"}); gap: 4.0.px; p: 6.0.px; rounded: $move ||px(if expanded(){20.0}else{28.0}); border_1; border_color: $move ||theme().border; bg: $move ||theme().card;
        } [
            $(input().element_ref(&editor).test_id("message-input").label(tr!("message")).placeholder(tr!("message")).multiline(true)
                .placeholder_color(||color(theme().muted_foreground))
                .row_start(1).col_span(move ||if expanded(){3_u16}else{1_u16}).col_start(move ||if expanded(){1_i16}else{2_i16})
                .min_w(px(0.0)).h_full().mx(move ||px(if expanded(){12.0}else{0.0})).py(editor_padding)
                .text_size(||px(preferences::state().with(|p|p.content_size))).value(||state().with(|s|s.draft.clone())).on_change(model::edit_draft)
                .on_focus(move ||focused.set(true)).on_blur(move ||focused.set(false))
                .disabled(||state().with(|s|s.session.is_none()||s.execution.submitting()))),
            .view { test_id: "composer-toolbar"; col_span: 3; col_start: 1; row_start: $move ||if expanded(){2_i16}else{1_i16}; h: 44.0.px; pointer_events: PointerEvents::BoxNone; flex_row; items_center; gap_1 } [
                $(attachment_menu(attachments)),
                $(when(expanded,move || model_options(model_open))),
                .view { flex_1; pointer_events: PointerEvents::None },
                $(when(expanded,move ||harness_menu(harness))),
                $(when(||state().with(|s|s.execution.running()),||ui::icon_action(ui::STOP,tr!("cancel"),model::cancel))),
                $(when(||state().with(|s|!s.execution.running()),||ui::action(model::send).accessibility_label(tr!("send")).size(px(44.0)).rounded_full().items_center()
                    .child(view().size(px(32.0)).rounded_full().items_center().justify_center().bg(||theme().primary).child(image(ui::SEND).size(px(18.0)).tint_color(||theme().primary_foreground)))
                    .disabled(||state().with(|s|s.session.is_none()||s.login_agent().is_some()||(s.draft.trim().is_empty()&&s.attachments.is_empty())||s.configuring||!s.execution.can_send())))),
            ],
        ],
    ] }
}
fn attachment_menu(open: Signal<bool>) -> impl IntoElement {
    ui::popup(open)
        .placement(Placement::Top)
        .align(Align::Start)
        .trigger(ui::trigger(ui::PLUS, tr!("attach")))
        .child(
            view()
                .child(
                    ui::labeled_action(ui::PHOTO, tr!("photos"), move || {
                        open.set(false);
                        data::attach_images();
                    })
                    .disabled(|| state().with(|s| s.session.is_none())),
                )
                .child(
                    ui::labeled_action(ui::FILE, tr!("files"), move || {
                        open.set(false);
                        data::attach();
                    })
                    .disabled(|| state().with(|s| s.session.is_none())),
                ),
        )
}
fn is_model(option: &acp::SessionConfigOption) -> bool {
    option.category == Some(acp::SessionConfigOptionCategory::Model)
        || option.id.to_string() == "model"
}
fn model_options(open: Signal<bool>) -> impl IntoElement {
    let options = computed(data::options);
    each(
        move || {
            options
                .get()
                .into_iter()
                .filter(is_model)
                .collect::<Vec<_>>()
        },
        |o| o.id.to_string(),
        move |option, _| dynamic(move || control(option.get(), Some(open))),
    )
}
fn harness_menu(open: Signal<bool>) -> impl IntoElement {
    let options = computed(data::options);
    ui::popup(open)
        .placement(Placement::Top)
        .trigger(ui::trigger(ui::SETTINGS, tr!("harness")))
        .child(
            view()
                .w(px(292.0))
                .child(
                    text(tr!("harness"))
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(|| theme().muted_foreground),
                )
                .child(
                    scroll().max_h(px(240.0)).child(
                        view()
                            .child(each(
                                move || {
                                    options
                                        .get()
                                        .into_iter()
                                        .filter(|o| !is_model(o))
                                        .collect::<Vec<_>>()
                                },
                                |o| o.id.to_string(),
                                |option, _| {
                                    dynamic(move || {
                                        let option = option.get();
                                        ui::SettingRow::default()
                                            .leading(Some(
                                                icon(option_icon(&option)).into_element(),
                                            ))
                                            .title(option.name.clone())
                                            .control(Some(control(option, None)))
                                            .render()
                                    })
                                },
                            ))
                            .child(when(|| data::options().is_empty(), legacy_mode)),
                    ),
                )
                .child(when(
                    || state().with(|s| s.usage.as_ref().is_some_and(|u| u.size > 0)),
                    context_usage,
                )),
        )
}
fn context_usage() -> impl IntoElement {
    let percent = || {
        state().with(|s| {
            s.usage
                .as_ref()
                .filter(|u| u.size > 0)
                .map(|u| (u.used as f64 / u.size as f64 * 100.0).clamp(0.0, 100.0) as f32)
                .unwrap_or_default()
        })
    };
    el! { .view { px_3; py_3; gap_2 } [
        .view { flex_row; justify_between; items_center } [
            $(text(tr!("context-window"))) { text_xs; text_color: $move || theme().muted_foreground },
            $(text(move || format!("{:.0}%", percent()))) { text_xs },
        ],
        $(nsh_base::progress().value(percent).accessibility_label(tr!("context-window"))
            .track_style(|s| s.h(px(6.0)).rounded_full().overflow_hidden().bg(|| theme().muted))
            .indicator_style(|s| s.h_full().rounded_full().bg(|| theme().primary))),
        $(text(|| state().with(|s| s.usage.as_ref().map(|u| format!("{} / {}", u.used, u.size)).unwrap_or_default()))) { text_xs; text_color: $move || theme().muted_foreground },
    ] }
}
fn legacy_mode() -> impl IntoElement {
    let control = picker(tr!("session-mode"))
        .items(|| {
            data::configuration()
                .and_then(|c| c.modes)
                .map(|m| {
                    m.available_modes
                        .iter()
                        .map(|v| SelectItem::new(v.id.to_string(), v.name.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .value(|| {
            data::configuration()
                .and_then(|c| c.modes)
                .map(|m| m.current_mode_id.to_string().into())
        })
        .on_change(data::set_mode)
        .trigger(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .h(px(44.0))
                .child(
                    text(|| {
                        data::configuration()
                            .and_then(|c| c.modes)
                            .map(|m| {
                                m.available_modes
                                    .iter()
                                    .find(|v| v.id == m.current_mode_id)
                                    .map(|v| v.name.clone())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default()
                    })
                    .text_size(crate::shared::ui::font_size),
                )
                .child(
                    image(ui::CHEVRON)
                        .size(px(12.0))
                        .tint_color(|| theme().muted_foreground),
                ),
        );
    ui::SettingRow::default()
        .leading(Some(icon(ui::SHIELD).into_element()))
        .title(tr!("session-mode"))
        .control(Some(control.into_element()))
        .render()
}
fn option_icon(option: &acp::SessionConfigOption) -> ImageAsset {
    match option.id.to_string().as_str() {
        "fast_mode" | "fast-mode" => return ui::FAST,
        "collaboration_mode" => return ui::WORKFLOW,
        _ => {}
    }
    match option.category {
        Some(acp::SessionConfigOptionCategory::Model) => ui::SPARKLES,
        Some(acp::SessionConfigOptionCategory::ThoughtLevel) => ui::THOUGHT,
        Some(acp::SessionConfigOptionCategory::Mode) => ui::SHIELD,
        _ => ui::SETTINGS,
    }
}
fn control(option: acp::SessionConfigOption, open: Option<Signal<bool>>) -> AnyElement {
    let glyph = is_model(&option).then(|| option_icon(&option));
    let id = option.id;
    match option.kind {
        acp::SessionConfigKind::Select(select) => {
            let choices: Vec<SelectItem> = match select.options {
                acp::SessionConfigSelectOptions::Ungrouped(options) => options
                    .iter()
                    .map(|o| SelectItem::new(o.value.to_string(), o.name.clone()))
                    .collect(),
                acp::SessionConfigSelectOptions::Grouped(groups) => groups
                    .iter()
                    .flat_map(|g| {
                        g.options.iter().map(|o| {
                            SelectItem::new(o.value.to_string(), o.name.clone())
                                .group(g.name.clone())
                        })
                    })
                    .collect(),
                _ => vec![],
            };
            let title = choices
                .iter()
                .find(|c| c.value.as_str() == select.current_value.to_string())
                .map(|c| c.title.to_string())
                .unwrap_or_else(|| select.current_value.to_string());
            picker(option.name).on_open_change(move |event: &nsh_base::SelectEvent| { if let nsh_base::SelectEvent::OpenChange(value, _) = event && let Some(open) = open { open.set(*value); } }).items(choices).value(Some(SharedString::from(select.current_value.to_string())))
                .disabled(|| state().with(|s| s.configuring))
                .on_change(move |value: &Option<SharedString>| if let Some(value) = value { data::configure(id.clone(), acp::SessionConfigOptionValue::value_id(value.to_string())); })
                .trigger(el! { .view { flex_row; items_center; gap_2; pl_2; pr: $(px(if glyph.is_some() { 8.0 } else { 0.0 })); max_w: 192.0.px; h: 44.0.px } [
                    $(glyph.map(icon)), title.text { text_size: $move ||px(preferences::state().with(|p|p.content_size)); line_height: 20.0.px; truncate; flex_shrink: 1.0 }, $(image(ui::CHEVRON).size(px(12.0)).flex_shrink_0().tint_color(|| theme().muted_foreground)),
                ] }).into_element()
        }
        acp::SessionConfigKind::Boolean(value) => ui::switch_control(
            option.name,
            move || value.current_value,
            move |checked| {
                data::configure(id.clone(), acp::SessionConfigOptionValue::boolean(*checked))
            },
        )
        .disabled(|| state().with(|s| s.configuring))
        .into_element(),
        _ => view().into_element(),
    }
}
