use crate::{features::connection::state, shared::ui};
use nsh_base::theme;
use nsh_core::prelude::*;

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
