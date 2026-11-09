use crate::{
    features::connection::{composer::Attachment, state},
    shared::{i18n::tr, ui},
};
use nsh_base::{SheetEvent, sheet, theme};
use nsh_core::prelude::*;

pub(super) fn render() -> impl IntoElement {
    let preview = signal(None::<Attachment>);
    view()
        .child(when(
            || state().with(|s| !s.attachments.is_empty()),
            move || {
                scroll()
                    .horizontal()
                    .h(px(44.0))
                    .mb_2()
                    .child(view().flex_row().gap_2().child(each(
                        || state().with(|s| s.attachments.clone()),
                        |a| a.id,
                        move |attachment, _| {
                            let menu = signal(false);
                            ui::context_menu(menu)
                                .trigger(
                                    view()
                                        .role(AccessibilityRole::Button)
                                        .on_long_press(move || menu.set(true))
                                        .label(move || attachment.with(|a| a.name.clone()))
                                        .on_press(move || {
                                            dismiss_keyboard();
                                            preview.set(Some(attachment.get()));
                                        })
                                        .rounded_lg()
                                        .bg(|| theme().muted)
                                        .child(
                                            ui::ControlRow::default()
                                                .leading(Some(
                                                    image(move || {
                                                        attachment.with(|a| {
                                                            if matches!(
                                                            a.content,
                                                            bas_client::acp::ContentBlock::Image(_)
                                                        ) {
                                                                ui::PHOTO
                                                            } else {
                                                                ui::FILE
                                                            }
                                                        })
                                                    })
                                                    .size(px(18.0))
                                                    .tint_color(|| theme().foreground)
                                                    .into_element(),
                                                ))
                                                .content(Some(
                                                    text(move || {
                                                        attachment.with(|a| a.name.clone())
                                                    })
                                                    .text_size(ui::font_size)
                                                    .max_w(px(180.0))
                                                    .truncate()
                                                    .into_element(),
                                                ))
                                                .render(),
                                        ),
                                )
                                .child(
                                    view()
                                        .child(ui::menu_action(
                                            ui::FILE,
                                            tr!("preview-attachment"),
                                            move || {
                                                menu.set(false);
                                                dismiss_keyboard();
                                                preview.set(Some(attachment.get()));
                                            },
                                        ))
                                        .child(
                                            ui::menu_action(
                                                ui::TRASH,
                                                tr!("remove-attachment"),
                                                move || {
                                                    let id = attachment.with(|a| a.id);
                                                    menu.set(false);
                                                    state().update(|s| {
                                                        s.attachments.retain(|a| a.id != id)
                                                    });
                                                },
                                            )
                                            .text_color(|| theme().destructive),
                                        ),
                                )
                        },
                    )))
            },
        ))
        .child(
            sheet()
                .side(Placement::Bottom)
                .backdrop_class("bg-black/30")
                .open(move || Some(preview.with(Option::is_some)))
                .on_open_change(move |e: &SheetEvent| {
                    if matches!(e, SheetEvent::OpenChange(false, _)) {
                        preview.set(None);
                    }
                })
                .child(dynamic(move || {
                    view()
                        .p_4()
                        .pb_safe_area_plus(px(16.0))
                        .gap_3()
                        .bg(|| theme().background)
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .child(
                                    text(move || {
                                        preview.with(|a| {
                                            a.as_ref().map(|a| a.name.clone()).unwrap_or_default()
                                        })
                                    })
                                    .flex_1()
                                    .truncate(),
                                )
                                .child(ui::icon_action(ui::BACK, tr!("dismiss"), move || {
                                    preview.set(None)
                                })),
                        )
                        .child(
                            scroll()
                                .max_h(move || window().height * 0.65)
                                .child(dynamic(move || {
                                    preview.get().map(|a| super::message::content(a.content))
                                })),
                        )
                })),
        )
}
