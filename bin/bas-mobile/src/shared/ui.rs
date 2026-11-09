use gds_uim::{component, el, m};
use nsh_base::{ButtonElement, SelectElement, button, select, theme};
use nsh_core::prelude::*;

pub(crate) const MENU: ImageAsset = asset!("icons/menu.svg");
pub(crate) const COMPOSE: ImageAsset = asset!("icons/square-pen.svg");
pub(crate) const FOLDER: ImageAsset = asset!("icons/folder.svg");
pub(crate) const CHEVRON: ImageAsset = asset!("icons/chevron-down.svg");
pub(crate) const CHAT: ImageAsset = asset!("icons/message-square.svg");
pub(crate) const BOT: ImageAsset = asset!("icons/bot.svg");
pub(crate) const MONITOR: ImageAsset = asset!("icons/monitor.svg");
pub(crate) const SETTINGS: ImageAsset = asset!("icons/settings-2.svg");
pub(crate) const SEND: ImageAsset = asset!("icons/arrow-up.svg");
pub(crate) const STOP: ImageAsset = asset!("icons/square.svg");
pub(crate) const REFRESH: ImageAsset = asset!("icons/refresh-cw.svg");
pub(crate) const PLUG: ImageAsset = asset!("icons/plug.svg");

pub(crate) fn icon(source: ImageAsset) -> impl IntoElement {
    image(source)
        .size(px(18.0))
        .flex_shrink_0()
        .tint_color(move || theme().foreground)
}

pub(crate) fn action(handler: impl Fn() + 'static) -> ButtonElement {
    button()
        .on_press(handler)
        .min_h(px(44.0))
        .justify_center()
        .rounded_lg()
        .hover(m! { bg: $move || theme().accent })
        .active(m! { bg: $move || theme().accent; opacity: 0.75 })
        .focus(m! { bg: $move || theme().accent })
        .disabled_style(m! { opacity: 0.35 })
}

pub(crate) fn icon_action(
    source: ImageAsset,
    label: String,
    handler: impl Fn() + 'static,
) -> ButtonElement {
    action(handler)
        .accessibility_label(label)
        .size(px(44.0))
        .items_center()
        .child(icon(source))
}

pub(crate) fn picker(label: String) -> SelectElement {
    select().accessibility_label(label).min_h(px(44.0)).margin(px(12.0)).offset(px(6.0))
        .list_style(m! { min_h: 44.0.px; max_h: $move ||px(264.0).min((window().height-keyboard_height()-px(96.0)).max(px(44.0))) })
        .popup_style(m! { min_w: 200.0.px; max_w: 320.0.px; mb_keyboard; p_1; pb_safe_area_plus: 4.0.px; rounded_lg; border_1; border_color: $move || theme().border; bg: $move || theme().popover; text_color: $move || theme().popover_foreground; shadow_md })
        .item_class("rounded-md focus:bg-muted selected:bg-accent disabled:opacity-40")
        .item_style(m! { h: 44.0.px; px_3; py_0; justify_center; text_size: font_size; line_height: 20.0.px })
        .backdrop_style(m! { bg: $(black().opacity(0.3)) })
}

#[component]
#[derive(Default)]
pub(crate) struct ControlRow {
    #[attr]
    leading: Option<AnyElement>,
    #[attr]
    content: Option<AnyElement>,
    #[attr]
    trailing: Option<AnyElement>,
}

impl ControlRow {
    pub fn render(self) -> impl IntoElement {
        el! { .view { flex_row; items_center; gap_3; px_3; min_h: 44.0.px; rounded: $move || theme().radius; hover: m! { bg: $move || theme().accent } } [
            $self.leading,
            .view { flex_1; min_w: 0.0.px } [$self.content],
            $self.trailing,
        ] }
    }
}

/// Reusable page anatomy: fixed header and footer around flexible content.
#[component]
#[derive(Default)]
pub(crate) struct ContentPage {
    #[attr]
    header: Option<AnyElement>,
    #[attr]
    content: Option<AnyElement>,
    #[attr]
    footer: Option<AnyElement>,
}

impl ContentPage {
    pub fn render(self) -> impl IntoElement {
        el! { .view { flex_1; min_h: 0.0.px } [
            $self.header,
            .view { flex_1; min_h: 0.0.px } [$self.content],
            $self.footer,
        ] }
    }
}

pub(crate) const PLUS: ImageAsset = asset!("icons/plus.svg");
pub(crate) const DOWNLOAD: ImageAsset = asset!("icons/download.svg");
pub(crate) const TRASH: ImageAsset = asset!("icons/trash.svg");
pub(crate) const ACTIVITY: ImageAsset = asset!("icons/activity.svg");
pub(crate) const LOGOUT: ImageAsset = asset!("icons/log-out.svg");
pub(crate) const LOGIN: ImageAsset = asset!("icons/log-in.svg");
pub(crate) const SAVE: ImageAsset = asset!("icons/save.svg");
pub(crate) const BACK: ImageAsset = asset!("icons/arrow-left.svg");
pub(crate) const SPARKLES: ImageAsset = asset!("icons/sparkles.svg");
pub(crate) const THOUGHT: ImageAsset = asset!("icons/lightbulb.svg");
pub(crate) const TOOL: ImageAsset = asset!("icons/wrench.svg");
pub(crate) const RUNNING: ImageAsset = asset!("icons/loader-circle.svg");
pub(crate) const SHIELD: ImageAsset = asset!("icons/shield.svg");
pub(crate) const ATTACH: ImageAsset = asset!("icons/paperclip.svg");
pub(crate) const TERMINAL: ImageAsset = asset!("icons/terminal.svg");

pub(crate) fn labeled_action(
    source: ImageAsset,
    label: String,
    handler: impl Fn() + 'static,
) -> ButtonElement {
    action(handler).accessibility_label(label.clone()).child(
        ControlRow::default()
            .leading(Some(icon(source).into_element()))
            .content(Some(
                text(label)
                    .text_size(crate::shared::ui::font_size)
                    .into_element(),
            ))
            .render(),
    )
}

pub(crate) fn field(
    label: String,
    value: impl Fn() -> String + 'static,
    change: impl Fn(&str) + 'static,
) -> nsh_core::element::Input {
    nsh_base::input()
        .label(label.clone())
        .placeholder(label)
        .placeholder_color(|| color(theme().muted_foreground))
        .value(value)
        .on_change(change)
        .min_h(px(44.0))
        .px_3()
        .py_2()
        .text_size(crate::shared::ui::font_size)
        .rounded_lg()
        .border_1()
        .border_color(move || theme().input)
        .bg(move || theme().background)
        .auto_capitalize(AutoCapitalize::None)
        .auto_correct(false)
}

pub(crate) fn switch_control(
    label: String,
    value: impl Fn() -> bool + 'static,
    change: impl Fn(&bool) + 'static,
) -> nsh_base::SwitchElement {
    nsh_base::switch()
        .accessibility_label(label.clone())
        .checked(value)
        .on_change(change)
        .flex_row()
        .items_center()
        .gap_2()
        .min_h(px(44.0))
        .track_style(|s| {
            s.w(px(40.0))
                .p(px(2.0))
                .rounded_full()
                .bg(move || theme().muted)
                .checked(|s| s.bg(move || theme().primary))
        })
        .thumb_style(|s| {
            s.size(px(18.0))
                .rounded_full()
                .bg(move || theme().background)
                .checked(|s| s.translate_x(px(18.0)))
        })
}

pub(crate) mod elicitation;

pub(crate) const EXTERNAL: ImageAsset = asset!("icons/external-link.svg");

pub(crate) const MORE: ImageAsset = asset!("icons/ellipsis-vertical.svg");
pub(crate) const INFO: ImageAsset = asset!("icons/info.svg");
pub(crate) const PHOTO: ImageAsset = asset!("icons/image.svg");
pub(crate) const FILE: ImageAsset = asset!("icons/file.svg");
pub(crate) const PALETTE: ImageAsset = asset!("icons/palette.svg");
pub(crate) const NEXT: ImageAsset = asset!("icons/chevron-right.svg");

pub(crate) fn popup(open: Signal<bool>) -> nsh_base::PopoverElement {
    nsh_base::popover().open(move || Some(open.get())).align(Align::End).offset(px(6.0)).margin(px(12.0))
        .on_open_change(move |event: &nsh_base::PopoverEvent| {
            if let nsh_base::PopoverEvent::OpenChange(value, _) = event { open.set(*value); }
        })
        .popup_style(m! { min_w: 200.0.px; max_w: 320.0.px; p_1; rounded_xl; border_1; border_color: $move || theme().border; bg: $move || theme().popover; text_color: $move || theme().popover_foreground; shadow_lg })
}
pub(crate) fn trigger(source: ImageAsset, label: String) -> impl IntoElement {
    view()
        .label(label)
        .size(px(44.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .child(icon(source))
}

#[component]
#[derive(Default)]
pub(crate) struct SettingsSection {
    #[attr]
    title: String,
    #[attr]
    content: Option<AnyElement>,
}
impl SettingsSection {
    pub fn render(self) -> impl IntoElement {
        el! { .view { gap_2; pb_4 } [
            $(text(self.title)) { text_size: crate::shared::ui::font_size; font_weight: FontWeight::SEMIBOLD; text_color: $move || theme().muted_foreground },
            .view { rounded_lg; bg: $move || theme().card; overflow_hidden } [$self.content],
        ] }
    }
}
#[component]
#[derive(Default)]
pub(crate) struct SettingRow {
    #[attr]
    leading: Option<AnyElement>,
    #[attr]
    title: String,
    #[attr]
    control: Option<AnyElement>,
}
impl SettingRow {
    pub fn render(self) -> impl IntoElement {
        el! { .view { flex_row; items_center; gap_3; min_h: 52.0.px; px_3; py_1; border_b_1; border_color: $move || theme().border } [
            $self.leading,
            $(text(self.title)) { flex_1; min_w: 0.0.px; text_size: font_size; line_height: 20.0.px },
            .view { flex_shrink_0; justify_center } [$self.control],
        ] }
    }
}
pub(crate) fn section(title: String, content: impl IntoElement) -> impl IntoElement {
    SettingsSection::default()
        .title(title)
        .content(Some(content.into_element()))
        .render()
}
pub(crate) fn setting(title: String, control: impl IntoElement) -> impl IntoElement {
    SettingRow::default()
        .title(title)
        .control(Some(control.into_element()))
        .render()
}

pub(crate) fn toggle(
    label: String,
    value: impl Fn() -> bool + 'static,
    change: impl Fn(&bool) + 'static,
) -> nsh_base::SwitchElement {
    switch_control(label.clone(), value, change)
        .child(text(label).text_size(crate::shared::ui::font_size))
}

pub(crate) const SEARCH: ImageAsset = asset!("icons/search.svg");

pub(crate) fn font_size() -> Points {
    px(super::preferences::state().with(|p| p.ui_size))
}

pub(crate) const CHECK: ImageAsset = asset!("icons/check.svg");
pub(crate) const FAILED: ImageAsset = asset!("icons/circle-x.svg");
pub(crate) const FAST: ImageAsset = asset!("icons/zap.svg");
pub(crate) const WORKFLOW: ImageAsset = asset!("icons/workflow.svg");

pub(crate) const BOOKMARK: ImageAsset = asset!("icons/bookmark.svg");
pub(crate) const PIN: ImageAsset = asset!("icons/pin.svg");
pub(crate) const ARCHIVE: ImageAsset = asset!("icons/archive.svg");
pub(crate) const ARCHIVE_RESTORE: ImageAsset = asset!("icons/archive-restore.svg");
pub(crate) const SERVER: ImageAsset = asset!("icons/server.svg");
pub(crate) const CHEVRON_UP: ImageAsset = asset!("icons/chevron-up.svg");
pub(crate) const CORNER_LEFT_UP: ImageAsset = asset!("icons/corner-left-up.svg");
pub(crate) const COPY: ImageAsset = asset!("icons/copy.svg");
pub(crate) const LIST_FILTER: ImageAsset = asset!("icons/list-filter.svg");
pub(crate) const CLOCK: ImageAsset = asset!("icons/clock.svg");

pub(crate) fn context_menu(open: Signal<bool>) -> nsh_base::ContextMenuElement {
    nsh_base::context_menu().open(move ||Some(open.get())).offset(px(6.0)).margin(px(12.0))
        .on_open_change(move |event: &nsh_base::MenuEvent| {
            if let nsh_base::MenuEvent::OpenChange(value, _) = event { open.set(*value); }
        })
        .popup_style(m! { min_w: 200.0.px; max_w: 320.0.px; p_1; rounded_xl; border_1; border_color: $move ||theme().border; bg: $move ||theme().popover; text_color: $move ||theme().popover_foreground; shadow_lg })
        .backdrop_class("bg-black/30")
}
pub(crate) fn menu_action(
    source: ImageAsset,
    label: String,
    handler: impl Fn() + 'static,
) -> nsh_base::MenuItemElement {
    nsh_base::menu_item()
        .accessibility_label(label.clone())
        .on_select(handler)
        .min_h(px(44.0))
        .rounded_lg()
        .hover(m! { bg: $move ||theme().accent })
        .focus(m! { bg: $move ||theme().accent })
        .child(
            ControlRow::default()
                .leading(Some(icon(source).into_element()))
                .content(Some(text(label).text_size(font_size).into_element()))
                .render(),
        )
}
