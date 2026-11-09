mod app;
mod entities;
mod features;
mod pages;
mod shared;
mod widgets;

use nsh_core::prelude::*;

#[native_module("bas/mobile")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
    registry.assets(assets!());
}

/// BAS mobile workspace and conversation shell.
#[component]
pub fn App() -> impl IntoElement {
    app::root()
}
