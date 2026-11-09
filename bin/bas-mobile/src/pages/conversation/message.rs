use crate::{
    features::connection::{Message, state},
    shared::{i18n::tr, preferences, ui},
};
use bas_client::acp;
use gds_uim::component;
use nsh_base::{DiffLayout, collapsible, diff_view, text_view, theme};
use nsh_core::prelude::*;

type Expanded = Signal<std::collections::HashMap<String, bool>>;
pub(super) fn render(
    message: impl Fn() -> Message + Copy + 'static,
    expanded: Expanded,
) -> impl IntoElement {
    view()
        .min_w(px(0.0))
        .gap_2()
        .child(when(
            move || message().activity(),
            move || activity_entry(message, expanded),
        ))
        .child(when(move || !message().activity(), move || body(message)))
}
fn body(message: impl Fn() -> Message + Copy + 'static) -> impl IntoElement {
    view()
        .min_w(px(0.0))
        .gap_2()
        .child(when(
            move || message().media.is_some(),
            move || dynamic(move || message().media.map(content)),
        ))
        .child(when(
            move || message().media.is_none(),
            move || {
                text_view()
                    .markdown(move || Some(message().body.into()))
                    .selectable(true)
            },
        ))
}
fn activity_entry(
    message: impl Fn() -> Message + Copy + 'static,
    expanded: Expanded,
) -> impl IntoElement {
    let header = element_ref();
    let tool = move || message().tool.is_some();
    let key = move || format!("entry:{}", message().sequence);
    let open = move || {
        expanded
            .with(|set| set.get(&key()).copied())
            .unwrap_or_else(|| preferences::state().with(|p| p.expand_tools))
    };
    let title = move || {
        if tool() {
            message()
                .body
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            tr!("thought-title")
        }
    };
    collapsible()
        .open(open)
        .w_full()
        .min_w(px(0.0))
        .rounded_lg()
        .border_1()
        .border_color(move || {
            if tool() {
                theme().border
            } else {
                AdaptiveColor::TRANSPARENT
            }
        })
        .child(
            ui::action(move || super::timeline::toggle(expanded, key(), open()))
                .accessibility_label(move || Some(SharedString::from(title())))
                .w_full()
                .min_w(px(0.0))
                .child(
                    view()
                        .test_id("activity-entry")
                        .element_ref(&header)
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .h(px(44.0))
                        .min_w(px(0.0))
                        .child(
                            image(move || if open() { ui::CHEVRON } else { ui::NEXT })
                                .size(px(14.0))
                                .flex_shrink_0()
                                .tint_color(|| theme().muted_foreground),
                        )
                        .child(
                            image(move || {
                                message()
                                    .tool
                                    .map(|t| tool_icon(t.kind))
                                    .unwrap_or(ui::THOUGHT)
                            })
                            .size(px(18.0))
                            .flex_shrink_0()
                            .tint_color(|| theme().muted_foreground),
                        )
                        .child(
                            text(title)
                                .test_id("activity-title")
                                .min_w(px(0.0))
                                .flex_1()
                                .truncate()
                                .text_size(|| px(preferences::state().with(|p| p.content_size)))
                                .line_height(px(20.0)),
                        )
                        .child(when(tool, move || {
                            view()
                                .size(px(18.0))
                                .flex_shrink_0()
                                .label(move || {
                                    message()
                                        .tool
                                        .map(|t| match t.status {
                                            acp::ToolCallStatus::Completed => tr!("tool-completed"),
                                            acp::ToolCallStatus::Failed => tr!("tool-failed"),
                                            acp::ToolCallStatus::InProgress => tr!("tool-running"),
                                            _ => tr!("tool-pending"),
                                        })
                                        .unwrap_or_default()
                                })
                                .child(
                                    image(move || {
                                        message()
                                            .tool
                                            .map(|t| match t.status {
                                                acp::ToolCallStatus::Completed => ui::CHECK,
                                                acp::ToolCallStatus::Failed => ui::FAILED,
                                                _ => ui::ACTIVITY,
                                            })
                                            .unwrap_or(ui::ACTIVITY)
                                    })
                                    .size(px(18.0))
                                    .tint_color(move || {
                                        if message().tool.is_some_and(|t| {
                                            t.status == acp::ToolCallStatus::Failed
                                        }) {
                                            theme().destructive
                                        } else {
                                            theme().muted_foreground
                                        }
                                    }),
                                )
                        })),
                ),
        )
        .content(when(open, move || {
            view()
                .px_3()
                .py_3()
                .gap_3()
                .child(when(tool, move || {
                    dynamic(move || message().tool.map(tool_content))
                }))
                .child(when(move || !tool(), move || body(message)))
                .child(super::navigation::footer(header, move || {
                    super::timeline::toggle(expanded, key(), true)
                }))
        }))
}
fn tool_content(tool: acp::ToolCall) -> impl IntoElement {
    let is_terminal = tool.kind == acp::ToolKind::Execute;
    // Structured content already represents the result. Its raw counterpart can
    // contain megabytes of image data and must not also become a text node.
    let rendered_output = tool.content.iter().any(|item| match item {
        acp::ToolCallContent::Content(_) | acp::ToolCallContent::Diff(_) => true,
        acp::ToolCallContent::Terminal(t) => state().with(|s| {
            s.terminals
                .get(t.terminal_id.0.as_ref())
                .is_some_and(|t| !t.output.is_empty())
        }),
        _ => false,
    });
    let mut children = view().gap_3();
    let mut input = tool.raw_input.clone();
    let command = input
        .as_mut()
        .and_then(|v| v.as_object_mut())
        .and_then(|v| v.remove("command"))
        .and_then(|v| v.as_str().map(str::to_owned));
    if let Some(command) = command {
        children = children.child(Command::default().code(command).render());
    }
    let input = input.filter(|v| !v.as_object().is_some_and(|o| o.is_empty()));
    for item in tool.content {
        children = match item {
            acp::ToolCallContent::Content(value) => children.child(match value.content {
                acp::ContentBlock::Text(text) if is_terminal => output(text.text),
                block => content(block),
            }),
            acp::ToolCallContent::Diff(diff) => children
                .child(text(diff.path.to_string_lossy().into_owned()).text_xs())
                .child(
                    diff_view()
                        .old_value(diff.old_text.unwrap_or_default())
                        .new_value(diff.new_text)
                        .layout(DiffLayout::Unified)
                        .soft_wrap(true)
                        .h(px(240.0)),
                ),
            acp::ToolCallContent::Terminal(t) => {
                children.child(terminal(t.terminal_id.to_string()))
            }
            _ => children,
        };
    }
    for (label, value) in [
        (tr!("tool-input"), input),
        (
            tr!("tool-output"),
            tool.raw_output.filter(|_| !rendered_output),
        ),
    ] {
        if let Some(value) = value {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| serde_json::to_string_pretty(&value).unwrap_or_default());
            children = children
                .child(
                    text(label)
                        .text_xs()
                        .text_color(|| theme().muted_foreground),
                )
                .child(output(value));
        }
    }
    children
}
pub(super) fn content(block: acp::ContentBlock) -> AnyElement {
    match block {
        acp::ContentBlock::Text(text) => text_view()
            .markdown(Some(SharedString::from(text.text)))
            .selectable(true)
            .into_element(),
        acp::ContentBlock::Image(img) => {
            image(format!("data:{};base64,{}", img.mime_type, img.data))
                .test_id("message-image")
                .object_fit(ResizeMode::Contain)
                .w_full()
                .h(px(240.0))
                .into_element()
        }
        acp::ContentBlock::Resource(resource) => match resource.resource {
            acp::EmbeddedResourceResource::TextResourceContents(resource) => view()
                .gap_1()
                .child(text(resource.uri).text_xs())
                .child(
                    text(resource.text)
                        .text_size(crate::shared::ui::font_size)
                        .selectable(true),
                )
                .into_element(),
            acp::EmbeddedResourceResource::BlobResourceContents(resource) => text(resource.uri)
                .text_size(crate::shared::ui::font_size)
                .into_element(),
            _ => view().into_element(),
        },
        acp::ContentBlock::ResourceLink(link) => {
            ui::labeled_action(ui::ATTACH, link.title.unwrap_or(link.name), move || {
                let uri = link.uri.clone();
                spawn(async move {
                    open_url(uri).await;
                });
            })
            .into_element()
        }
        acp::ContentBlock::Audio(audio) => text(format!("{} · {}", tr!("audio"), audio.mime_type))
            .text_size(crate::shared::ui::font_size)
            .into_element(),
        _ => view().into_element(),
    }
}

fn output(value: String) -> AnyElement {
    Output::default().code(value).render().into_element()
}

fn terminal(id: String) -> impl IntoElement {
    let terminal = state()
        .with(|s| s.terminals.get(&id).cloned())
        .unwrap_or_default();
    view()
        .gap_2()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(ui::icon(ui::TERMINAL))
                .child(text(tr!("terminal-output")).text_xs().flex_1())
                .child(
                    terminal
                        .exit_code
                        .map(|code| text(tr!("terminal-exit", code as usize)).text_xs()),
                )
                .child(
                    terminal.signal.map(|signal| {
                        text(format!("{}: {signal}", tr!("terminal-signal"))).text_xs()
                    }),
                ),
        )
        .child(
            terminal
                .cwd
                .map(|cwd| text(cwd).text_xs().text_color(|| theme().muted_foreground)),
        )
        .child((!terminal.output.is_empty()).then(|| output(terminal.output)))
}

#[component]
#[derive(Default)]
struct Output {
    #[attr]
    code: String,
}
impl Output {
    fn render(self) -> impl IntoElement {
        dynamic(move || {
            let code = text(self.code.clone())
                .p_3()
                .font_family(|| {
                    theme()
                        .font_mono
                        .clone()
                        .unwrap_or_else(|| "monospace".into())
                })
                .text_size(|| px(preferences::state().with(|p| p.code_size)))
                .line_height(|| px(preferences::state().with(|p| p.code_size * p.line_height)))
                .selectable(true);
            let body = if preferences::state().with(|p| p.wrap_output) {
                code.into_element()
            } else {
                scroll()
                    .horizontal()
                    .max_h(px(240.0))
                    .child(code)
                    .into_element()
            };
            view()
                .test_id("tool-output-code")
                .min_w(px(0.0))
                .rounded_lg()
                .border_1()
                .border_color(|| theme().border)
                .bg(|| theme().muted)
                .overflow_hidden()
                .child(body)
        })
    }
}

#[component]
#[derive(Default)]
struct Command {
    #[attr]
    code: String,
}
impl Command {
    fn render(self) -> impl IntoElement {
        use nsh_base::text_view::{Block, CodeBlock, Document};
        let copy = self.code.clone();
        view()
            .test_id("tool-command")
            .gap_2()
            .min_w(px(0.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(ui::icon(ui::TERMINAL))
                    .child(text(tr!("command-input")).text_xs().flex_1())
                    .child(ui::icon_action(ui::COPY, tr!("copy-command"), move || {
                        set_clipboard(copy.clone())
                    })),
            )
            .child(
                text_view()
                    .document(Some(Document::new(vec![Block::CodeBlock(CodeBlock::new(
                        Some("bash".into()),
                        self.code,
                    ))])))
                    .selectable(true),
            )
    }
}

fn tool_icon(kind: acp::ToolKind) -> ImageAsset {
    match kind {
        acp::ToolKind::Read => ui::FILE,
        acp::ToolKind::Edit => ui::COMPOSE,
        acp::ToolKind::Delete => ui::TRASH,
        acp::ToolKind::Search => ui::SEARCH,
        acp::ToolKind::Execute => ui::TERMINAL,
        acp::ToolKind::Think => ui::THOUGHT,
        acp::ToolKind::Fetch => ui::DOWNLOAD,
        _ => ui::TOOL,
    }
}
