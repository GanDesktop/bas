use crate::{features::connection::state, shared::ui};
use nsh_base::theme;
use nsh_core::prelude::*;

pub(crate) fn login_status(agent: impl Fn() -> String + 'static) -> impl IntoElement {
    use crate::{
        entities::agent::{Key, LoginStatus},
        shared::{api, i18n::tr},
    };
    let status = computed(move || {
        api::context()
            .map(|context| Key::new(context.backend, agent()).status())
            .unwrap_or_default()
    });
    when(
        move || status.get() != LoginStatus::Unknown,
        move || {
            text(move || match status.get() {
                LoginStatus::Ready => tr!("auth-complete"),
                LoginStatus::Required => tr!("auth-required"),
                LoginStatus::Unknown => String::new(),
            })
            .text_xs()
            .text_color(|| theme().muted_foreground)
        },
    )
}

pub(crate) fn icon(agent: impl Fn() -> String + 'static) -> impl IntoElement {
    image(move || {
        state()
            .with(|s| s.agent_icons.get(&agent()).cloned())
            .unwrap_or_else(|| ui::BOT.url.into())
    })
    .size(px(18.0))
    .flex_shrink_0()
    .object_fit(ResizeMode::Contain)
    .tint_color(|| theme().foreground)
    .placeholder(ui::BOT)
}
