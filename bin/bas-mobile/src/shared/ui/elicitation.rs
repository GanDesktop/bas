use super::{BACK, CHEVRON, MONITOR, SAVE, field, labeled_action, picker, toggle};
use crate::shared::i18n::tr;
use bas_client::authentication::elicitation::{Field, Kind, Mode, Request};
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;
use serde_json::{Map, Value, json};
use std::rc::Rc;

pub(crate) fn render(request: Request, respond: impl Fn(Value) + 'static) -> impl IntoElement {
    let response = Rc::new(respond);
    let values = signal(match &request.mode {
        Mode::Form(form) => form.defaults(),
        _ => Map::new(),
    });
    let error = signal(String::new());
    let submit = response.clone();
    let decline = response;
    let check = request.clone();
    let mut content = view()
        .gap_2()
        .child(text(request.message).text_size(crate::shared::ui::font_size));
    match request.mode {
        Mode::Url(url) => {
            content = content.child(labeled_action(MONITOR, url.host, move || {
                let url = url.url.clone();
                spawn(async move {
                    open_url(url).await;
                });
            }));
        }
        Mode::Form(form) => {
            for property in form.fields {
                content = content.child(render_field(property, values));
            }
        }
    }
    content
        .child(
            text(move || error.get())
                .text_size(crate::shared::ui::font_size)
                .text_color(|| theme().destructive),
        )
        .child(
            view()
                .flex_row()
                .justify_end()
                .gap_2()
                .child(labeled_action(BACK, tr!("decline"), move || {
                    decline(json!({"action":"decline"}))
                }))
                .child(labeled_action(SAVE, tr!("continue"), move || {
                    let value = match check.mode {
                        Mode::Form(_) => json!({"action":"accept", "content": values.get()}),
                        Mode::Url(_) => json!({"action":"accept"}),
                    };
                    match bas_client::authentication::elicitation::validate_response(&check, &value)
                    {
                        Ok(()) => {
                            error.set(String::new());
                            submit(value);
                        }
                        Err(e) => error.set(e.to_string()),
                    }
                })),
        )
}
fn render_field(property: Field, values: Signal<Map<String, Value>>) -> AnyElement {
    let name = property.name.clone();
    let read = name.clone();
    let control = match property.kind {
        Kind::Boolean => toggle(
            property.title.clone(),
            move || values.with(|v| v.get(&read).and_then(Value::as_bool).unwrap_or(false)),
            move |value| {
                values.update(|v| {
                    v.insert(name.clone(), json!(value));
                })
            },
        )
        .into_element(),
        Kind::Enum { options } => picker(property.title.clone())
            .items(
                options
                    .iter()
                    .map(|o| SelectItem::new(o.value.clone(), o.title.clone()))
                    .collect::<Vec<_>>(),
            )
            .value(move || {
                values.with(|v| {
                    v.get(&read)
                        .and_then(Value::as_str)
                        .map(|v| SharedString::from(v.to_owned()))
                })
            })
            .on_change(move |value: &Option<SharedString>| {
                values.update(|v| {
                    if let Some(value) = value {
                        v.insert(name.clone(), json!(value.as_str()));
                    } else {
                        v.remove(&name);
                    }
                })
            })
            .trigger(
                view()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .min_h(px(44.0))
                    .px_3()
                    .border_1()
                    .border_color(|| theme().input)
                    .rounded_lg()
                    .child(
                        text({
                            let name = property.name.clone();
                            let title = property.title.clone();
                            move || {
                                values.with(|v| {
                                    v.get(&name)
                                        .and_then(Value::as_str)
                                        .unwrap_or(&title)
                                        .to_owned()
                                })
                            }
                        })
                        .text_size(crate::shared::ui::font_size),
                    )
                    .child(super::icon(CHEVRON)),
            )
            .into_element(),
        Kind::MultiSelect { options } => {
            let mut group = view().gap_1();
            for option in options {
                let name = name.clone();
                let read = name.clone();
                let selected = option.value.clone();
                let choice = option.value;
                group = group.child(toggle(
                    option.title,
                    move || {
                        values.with(|v| {
                            v.get(&read).and_then(Value::as_array).is_some_and(|items| {
                                items.iter().any(|v| v.as_str() == Some(&selected))
                            })
                        })
                    },
                    move |checked| {
                        values.update(|v| {
                            let mut items = v
                                .get(&name)
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_default();
                            items.retain(|v| v.as_str() != Some(&choice));
                            if *checked {
                                items.push(json!(choice));
                            }
                            v.insert(name.clone(), Value::Array(items));
                        })
                    },
                ));
            }
            group.into_element()
        }
        kind => field(
            property.title.clone(),
            move || {
                values.with(|v| {
                    v.get(&read)
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .unwrap_or_default()
                })
            },
            move |text| {
                values.update(|v| {
                    if text.is_empty() {
                        v.remove(&name);
                        return;
                    }
                    let value = match kind {
                        Kind::Integer => text
                            .parse::<i64>()
                            .map(|n| json!(n))
                            .unwrap_or_else(|_| json!(text)),
                        Kind::Number => text
                            .parse::<f64>()
                            .ok()
                            .and_then(serde_json::Number::from_f64)
                            .map(Value::Number)
                            .unwrap_or_else(|| json!(text)),
                        _ => json!(text),
                    };
                    v.insert(name.clone(), value);
                })
            },
        )
        .into_element(),
    };
    view()
        .gap_1()
        .child(
            text(format!(
                "{}{}",
                property.title,
                if property.required { " *" } else { "" }
            ))
            .text_size(crate::shared::ui::font_size),
        )
        .child(control)
        .child(
            property
                .description
                .map(|v| text(v).text_xs().text_color(|| theme().muted_foreground)),
        )
        .into_element()
}
