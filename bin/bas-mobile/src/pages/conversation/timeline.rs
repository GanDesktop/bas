use super::message;
use crate::{
    features::connection::{Message, timeline as data},
    shared::{i18n::tr, preferences, ui},
};
use gds_uim::{component, el};
use nsh_base::{collapsible, theme};
use nsh_core::prelude::*;
use std::collections::HashMap;

type Expanded = Signal<HashMap<String, bool>>;
pub(super) fn toggle(expanded: Expanded, key: String, open: bool) {
    expanded.update(|set| {
        set.insert(key, !open);
    });
}
pub(super) fn render() -> impl IntoElement {
    let rows = computed(data::rows);
    let navigation =
        use_context::<super::navigation::Navigation>().expect("conversation navigation");
    let expanded = signal(HashMap::new());
    let follow = computed(|| preferences::state().with(|p| p.follow_messages));
    view()
        .flex_row()
        .flex_1()
        .min_h(px(0.0))
        .child(dynamic(move || {
            let following = follow.get();
            let list = virtual_list(
                move || rows.with(Vec::len),
                move |at| {
                    let row = move || rows.with(|r| r.get(at.index()).cloned());
                    let messages = move || row().map(|r| data::messages(&r)).unwrap_or_default();
                    view()
                        .pl_4()
                        .pr_1()
                        .py_1()
                        .min_w(px(0.0))
                        .child(when(
                            move || row().is_some_and(|r| r.activity),
                            move || activity(messages, expanded),
                        ))
                        .child(when(
                            move || row().is_some_and(|r| !r.activity),
                            move || {
                                view()
                                    .py_2()
                                    .px_3()
                                    .rounded_xl()
                                    .min_w(px(0.0))
                                    .bg(move || {
                                        if messages().first().is_some_and(|m| m.role == "user") {
                                            theme().muted
                                        } else {
                                            AdaptiveColor::TRANSPARENT
                                        }
                                    })
                                    .child(message::render(
                                        move || messages().into_iter().next().unwrap_or_default(),
                                        expanded,
                                    ))
                            },
                        ))
                },
            )
            .keys(move || rows.with(|r| r.iter().map(|r| r.key.clone()).collect::<Vec<_>>()))
            .element_ref(&navigation.list)
            .on_scroll(move |e: &ScrollEvent| navigation.offset.set(e.offset.y))
            .on_viewable_items_change(move |r: &ViewableItems| navigation.visible(r))
            .content_padding_y(px(8.0))
            .flex_1();
            if following {
                list.follow_end(px(80.0))
            } else {
                list
            }
        }))
        .child(super::navigation::render(navigation))
}
fn visible(messages: impl Fn() -> Vec<Message>) -> Vec<Message> {
    let thoughts = preferences::state().with(|p| p.show_thoughts);
    messages()
        .into_iter()
        .filter(|m| thoughts || m.role != "thought")
        .collect()
}
#[derive(Clone, PartialEq)]
struct Summary {
    key: &'static str,
    count: usize,
    icon: ImageAsset,
}
impl Summary {
    fn label(&self) -> String {
        tr!(self.key, self.count)
    }
    fn color(&self) -> AdaptiveColor {
        match self.key {
            "running-count" => theme().primary,
            "failed-count" => theme().destructive,
            _ => theme().muted_foreground,
        }
    }
}

#[component]
#[derive(Default)]
struct ActivityCount {
    #[attr]
    icon: Option<AnyElement>,
    #[attr]
    label: Option<AnyElement>,
}
impl ActivityCount {
    fn render(self) -> impl IntoElement {
        el! { .view { flex_row; items_center; flex_shrink_0; gap_1 } [
            $self.icon,
            $self.label,
        ] }
    }
}

fn summary(messages: &[Message]) -> Vec<Summary> {
    let thoughts = messages.iter().filter(|m| m.role == "thought").count();
    let tools = messages.iter().filter(|m| m.tool.is_some()).count();
    let running = messages
        .iter()
        .filter(|m| {
            m.tool.as_ref().is_some_and(|t| {
                matches!(
                    t.status,
                    bas_client::acp::ToolCallStatus::Pending
                        | bas_client::acp::ToolCallStatus::InProgress
                )
            })
        })
        .count();
    let failed = messages
        .iter()
        .filter(|m| {
            m.tool
                .as_ref()
                .is_some_and(|t| t.status == bas_client::acp::ToolCallStatus::Failed)
        })
        .count();
    [
        Summary {
            key: "thought-count",
            count: thoughts,
            icon: ui::THOUGHT,
        },
        Summary {
            key: "tool-count",
            count: tools,
            icon: ui::TOOL,
        },
        Summary {
            key: "running-count",
            count: running,
            icon: ui::RUNNING,
        },
        Summary {
            key: "failed-count",
            count: failed,
            icon: ui::FAILED,
        },
    ]
    .into_iter()
    .filter(|entry| entry.count > 0)
    .collect()
}
fn activity(
    messages: impl Fn() -> Vec<Message> + Copy + 'static,
    expanded: Expanded,
) -> impl IntoElement {
    let header = element_ref();
    let contents = move || visible(messages);
    let grouped = move || contents().len() > 1;
    let key = move || {
        format!(
            "group:{}",
            messages().first().map(|m| m.sequence).unwrap_or_default()
        )
    };
    let open = move || {
        expanded
            .with(|set| set.get(&key()).copied())
            .unwrap_or_else(|| {
                preferences::state().with(|p| p.expand_tools)
                    || contents().iter().any(|m| {
                        expanded.with(|set| {
                            set.get(&format!("entry:{}", m.sequence))
                                .copied()
                                .unwrap_or(false)
                        })
                    })
            })
    };
    view()
        .min_w(px(0.0))
        .child(when(grouped, move || {
            collapsible()
                .open(open)
                .child(
                    ui::action(move || toggle(expanded, key(), open()))
                        .accessibility_label(move || {
                            Some(SharedString::from(
                                summary(&contents())
                                    .iter()
                                    .map(Summary::label)
                                    .collect::<Vec<_>>()
                                    .join(" · "),
                            ))
                        })
                        .w_full()
                        .min_w(px(0.0))
                        .child(
                            view()
                                .test_id("activity-summary")
                                .element_ref(&header)
                                .flex_row()
                                .items_center()
                                .px_3()
                                .py_2()
                                .gap_2()
                                .child(
                                    image(move || if open() { ui::CHEVRON } else { ui::NEXT })
                                        .size(px(14.0))
                                        .flex_shrink_0()
                                        .tint_color(|| theme().muted_foreground),
                                )
                                .child(
                                    view()
                                        .flex_row()
                                        .flex_wrap()
                                        .items_center()
                                        .gap_x_3()
                                        .gap_y_1()
                                        .min_w(px(0.0))
                                        .flex_1()
                                        .child(each(
                                            move || summary(&contents()),
                                            |entry| entry.key,
                                            |entry, _| {
                                                ActivityCount::default()
                                                    .icon(Some(
                                                        image(move || entry.with(|e| e.icon))
                                                            .size(px(16.0))
                                                            .flex_shrink_0()
                                                            .tint_color(move || {
                                                                entry.with(Summary::color)
                                                            })
                                                            .into_element(),
                                                    ))
                                                    .label(Some(
                                                        text(move || entry.with(Summary::label))
                                                            .text_size(|| {
                                                                px(preferences::state()
                                                                    .with(|p| p.content_size))
                                                            })
                                                            .line_height(px(20.0))
                                                            .text_color(move || {
                                                                entry.with(Summary::color)
                                                            })
                                                            .into_element(),
                                                    ))
                                                    .render()
                                            },
                                        )),
                                ),
                        ),
                )
                .content(when(open, move || {
                    view()
                        .pt_3()
                        .ml_3()
                        .pl_2()
                        .border_l_1()
                        .border_color(|| theme().border)
                        .gap_1()
                        .child(each(
                            move || contents().iter().map(|m| m.sequence).collect::<Vec<_>>(),
                            |id| *id,
                            move |sequence, _| {
                                message::render(
                                    move || {
                                        contents()
                                            .into_iter()
                                            .find(|m| m.sequence == sequence.get())
                                            .unwrap_or_default()
                                    },
                                    expanded,
                                )
                            },
                        ))
                        .child(super::navigation::footer(header, move || {
                            toggle(expanded, key(), true)
                        }))
                }))
        }))
        .child(when(
            move || !grouped() && !contents().is_empty(),
            move || {
                message::render(
                    move || contents().into_iter().next().unwrap_or_default(),
                    expanded,
                )
            },
        ))
}
