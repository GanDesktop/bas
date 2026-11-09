use crate::{
    features::connection::{navigation as marks, state, timeline as data},
    shared::{i18n::tr, time, ui},
};
use nsh_base::{SheetEvent, sheet, theme};
use nsh_core::prelude::*;

const TICK_HEIGHT: f32 = 12.0;
#[derive(Clone, Copy)]
pub(super) struct Navigation {
    pub list: ElementRef,
    pub offset: Signal<Points>,
    pub active: Signal<Option<u64>>,
    visible_cursor: Signal<Option<u64>>,
    open: Signal<bool>,
    bookmarks: Signal<bool>,
}
impl Navigation {
    pub fn new() -> Self {
        Self {
            list: element_ref(),
            offset: signal(Points::ZERO),
            active: signal(None),
            visible_cursor: signal(None),
            open: signal(false),
            bookmarks: signal(false),
        }
    }
    pub fn jump(self, cursor: u64) {
        let index = data::rows()
            .iter()
            .position(|r| data::messages(r).iter().any(|m| m.sequence >= cursor));
        if let Some(index) = index {
            self.active.set(Some(cursor));
            self.list
                .scroll_to_index(index as u32, percentage(0.0), px(0.0), false);
        }
    }
    pub fn visible(self, rows: &ViewableItems) {
        let Some((first, _)) = rows.viewable else {
            return;
        };
        let cursor = data::rows()
            .get(first)
            .and_then(|r| data::messages(r).first().map(|m| m.sequence));
        self.visible_cursor.set(cursor);
    }
    pub fn top(self, header: ElementRef, collapse: impl FnOnce() + 'static) {
        spawn(async move {
            let Some(header) = header.measure().await else {
                return;
            };
            let Some(list) = self.list.measure().await else {
                return;
            };
            let target =
                (self.offset.get_untracked() + header.page.y - list.page.y).max(Points::ZERO);
            collapse();
            self.list.scroll_to_offset(target, false);
        });
    }
}
pub(super) fn footer(header: ElementRef, collapse: impl Fn() + Copy + 'static) -> impl IntoElement {
    let navigation = use_context::<Navigation>().expect("timeline navigation");
    view()
        .flex_row()
        .justify_end()
        .gap_1()
        .pt_2()
        .child(ui::icon_action(
            ui::CORNER_LEFT_UP,
            tr!("content-top"),
            move || navigation.top(header, || {}),
        ))
        .child(ui::icon_action(
            ui::CHEVRON_UP,
            tr!("collapse-content"),
            move || navigation.top(header, collapse),
        ))
}
pub(super) fn render(navigation: Navigation) -> impl IntoElement {
    let open = navigation.open;
    let marker = computed(move || {
        let cursor = navigation.visible_cursor.get()?;
        state().with(|s| {
            s.overview
                .iter()
                .rev()
                .find(|e| e.cursor <= cursor)
                .map(|e| e.cursor)
        })
    });
    effect(move || navigation.active.set(marker.get()));
    let start = stored((Points::ZERO, 0_usize));
    let ticks = element_ref();
    effect(move || {
        let current = navigation.active.get();
        let index = state()
            .with(|s| s.overview.iter().position(|e| Some(e.cursor) == current))
            .unwrap_or_default();
        ticks.scroll_to_offset(px((index as f32 * TICK_HEIGHT - 108.0).max(0.0)), false);
    });
    view().w(px(32.0)).justify_center().child(
        view()
            .test_id("turn-rail")
            .label(tr!("turn-navigation"))
            .role(AccessibilityRole::Button)
            .on_long_press(move || {
                dismiss_keyboard();
                open.set(true);
            })
            .on_press(move || {
                dismiss_keyboard();
                open.set(true);
            })
            .claim_touches(TouchClaim::on_pan(PanActivation::vertical(
                px(4.0),
                px(20.0),
            )))
            .on_responder_grant(move |e: &ResponderEvent| {
                let index = state()
                    .with(|s| {
                        s.overview
                            .iter()
                            .position(|e| Some(e.cursor) == navigation.active.get_untracked())
                    })
                    .unwrap_or_default();
                start.replace((e.page.y, index));
            })
            .on_responder_move(move |e: &ResponderEvent| {
                let (y, index) = start.get();
                let target = (index as f32 + f32::from(e.page.y - y) / TICK_HEIGHT)
                    .round()
                    .max(0.0) as usize;
                if let Some(cursor) = state().with(|s| {
                    s.overview
                        .get(target.min(s.overview.len().saturating_sub(1)))
                        .map(|e| e.cursor)
                }) {
                    navigation.jump(cursor);
                }
            })
            .py_3()
            .rounded_lg()
            .active(|s| s.bg(theme().muted))
            .child(
                scroll()
                    .element_ref(&ticks)
                    .scrollable(false)
                    .scrollbar(false)
                    .max_h(px(240.0))
                    .pointer_events(PointerEvents::None)
                    .child(each(
                        || state().with(|s| s.overview.clone()),
                        |e| e.cursor,
                        move |entry, _| {
                            view()
                                .h(px(TICK_HEIGHT))
                                .items_center()
                                .justify_center()
                                .child(
                                    view()
                                        .w(move || {
                                            px(
                                                if Some(entry.with(|e| e.cursor))
                                                    == navigation.active.get()
                                                {
                                                    20.0
                                                } else {
                                                    12.0
                                                },
                                            )
                                        })
                                        .h(move || {
                                            px(
                                                if Some(entry.with(|e| e.cursor))
                                                    == navigation.active.get()
                                                {
                                                    3.0
                                                } else {
                                                    2.0
                                                },
                                            )
                                        })
                                        .rounded_full()
                                        .bg(move || {
                                            let cursor = entry.with(|e| e.cursor);
                                            if Some(cursor) == navigation.active.get()
                                                || state().with(|s| {
                                                    marks::turn_bookmarked(
                                                        s.session.as_deref().unwrap_or_default(),
                                                        cursor,
                                                    )
                                                })
                                            {
                                                theme().primary
                                            } else {
                                                theme().muted_foreground
                                            }
                                        }),
                                )
                        },
                    )),
            ),
    )
}

pub(super) fn panel(navigation: Navigation) -> impl IntoElement {
    let open = navigation.open;
    let bookmarks = navigation.bookmarks;
    let entries = move || {
        state().with(|s| {
            s.overview
                .iter()
                .filter(|e| {
                    !bookmarks.get()
                        || marks::turn_bookmarked(
                            s.session.as_deref().unwrap_or_default(),
                            e.cursor,
                        )
                })
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    sheet()
        .side(Placement::Right)
        .backdrop_class("bg-black/30")
        .popup_style(|s| {
            s.w(move || (window().width - px(24.0)).min(px(380.0)))
                .bg(|| theme().background)
        })
        .open(move || Some(open.get()))
        .on_open_change(move |e: &SheetEvent| {
            if let SheetEvent::OpenChange(value, _) = e {
                open.set(*value);
            }
        })
        .child(dynamic(move || {
            view()
                .w_full()
                .flex_1()
                .py_safe_area()
                .bg(|| theme().background)
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .px_3()
                        .py_2()
                        .child(
                            text(tr!("turn-navigation"))
                                .flex_1()
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            ui::icon_action(ui::BOOKMARK, tr!("bookmarked-turns"), move || {
                                bookmarks.update(|v| *v = !*v)
                            })
                            .selected(bookmarks)
                            .selected_style(|s| s.bg(theme().accent)),
                        )
                        .child(ui::icon_action(ui::BACK, tr!("dismiss"), move || {
                            open.set(false)
                        })),
                )
                .child(when(
                    move || entries().is_empty(),
                    || {
                        text(tr!("no-turns"))
                            .p_4()
                            .text_color(|| theme().muted_foreground)
                    },
                ))
                .child(
                    virtual_list(
                        move || entries().len(),
                        move |at| {
                            let entry = move || entries().get(at.index()).cloned();
                            view()
                                .mx_3()
                                .py_2()
                                .border_b_1()
                                .border_color(|| theme().border)
                                .flex_row()
                                .items_start()
                                .child(
                                    ui::action(move || {
                                        if let Some(e) = entry() {
                                            open.set(false);
                                            navigation.jump(e.cursor);
                                        }
                                    })
                                    .accessibility_label(move || {
                                        Some(SharedString::from(tr!("jump-turn", at.index() + 1)))
                                    })
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        view()
                                            .p_2()
                                            .gap_2()
                                            .child(
                                                text(move || {
                                                    entry().map(|e| e.preview).unwrap_or_default()
                                                })
                                                .text_size(ui::font_size),
                                            )
                                            .child(
                                                text(move || {
                                                    entry()
                                                        .map(|e| time::stamp(e.timestamp_millis))
                                                        .unwrap_or_default()
                                                })
                                                .text_xs()
                                                .text_color(|| theme().muted_foreground),
                                            ),
                                    ),
                                )
                                .child(
                                    ui::icon_action(
                                        ui::BOOKMARK,
                                        tr!("toggle-turn-bookmark"),
                                        move || {
                                            if let Some(e) = entry()
                                                && let Some(session) =
                                                    state().with(|s| s.session.clone())
                                            {
                                                marks::toggle_turn(&session, e.cursor);
                                            }
                                        },
                                    )
                                    .accessibility_label(format!(
                                        "{} {}",
                                        tr!("toggle-turn-bookmark"),
                                        at.index() + 1
                                    ))
                                    .selected(move || {
                                        entry().is_some_and(|e| {
                                            state().with(|s| {
                                                marks::turn_bookmarked(
                                                    s.session.as_deref().unwrap_or_default(),
                                                    e.cursor,
                                                )
                                            })
                                        })
                                    })
                                    .selected_style(|s| s.bg(theme().accent)),
                                )
                        },
                    )
                    .flex_1(),
                )
        }))
}
