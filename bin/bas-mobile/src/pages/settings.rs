mod agents;
mod connections;
mod frontend;
mod policy;

use crate::{
    features::agent_management,
    shared::{
        i18n::tr,
        preferences,
        routes::{self, Page},
        ui::{self, ContentPage, ControlRow, icon},
    },
    widgets::backend,
};
use gds_uim::el;
use nsh_base::theme;
use nsh_core::prelude::*;

pub(crate) fn render(page: Page) -> impl IntoElement {
    let header = el! { .view { px_2; py_1; flex_row; items_center; gap_2; border_b_1; border_color: $move ||theme().border } [
        $(ui::icon_action(ui::BACK,tr!("back"),routes::back)),
        $(text(move || title(page))) { flex_1; font_weight: FontWeight::SEMIBOLD },
        $(when(move ||page==Page::Appearance,||ui::icon_action(ui::REFRESH,tr!("reset"),preferences::reset_appearance))),
        $(when(move ||page==Page::AgentEditor,||ui::labeled_action(ui::SAVE,tr!("save"),agent_management::save).disabled(||agent_management::state().with(|s|s.busy||s.configuration.is_none())))),
        $(when(move ||page==Page::Policy,||text(||crate::features::session_policy::state().with(|s|if s.busy {tr!("saving")}else if s.saved{tr!("saved")}else{String::new()})).text_xs())),
    ] };
    let content = match page {
        Page::Settings => overview().into_element(),
        Page::Appearance => frontend::appearance().into_element(),
        Page::ChatPreferences => frontend::chat().into_element(),
        Page::Connections => connections::render().into_element(),
        Page::Terminal => frontend::terminal().into_element(),
        Page::About => about().into_element(),
        Page::Agents => agents::list(false).into_element(),
        Page::Catalog => agents::list(true).into_element(),
        Page::AgentDetail => agents::detail().into_element(),
        Page::AgentEditor => agents::editor().into_element(),
        Page::Policy => policy::render().into_element(),
        Page::Conversation | Page::Authentication => view().into_element(),
    };
    ContentPage::default()
        .header(Some(header.into_element()))
        .content(Some(content))
        .render()
}
fn title(page: Page) -> String {
    tr!(match page {
        Page::Settings => "settings",
        Page::Appearance => "appearance",
        Page::ChatPreferences => "chat-preferences",
        Page::Connections => "connections",
        Page::Terminal => "terminal-settings",
        Page::About => "about",
        Page::Agents => "agents",
        Page::Catalog => "add-agent",
        Page::AgentDetail => "agent-details",
        Page::AgentEditor => "custom-agent",
        Page::Policy => "session-policy",
        Page::Conversation => "sessions",
        Page::Authentication => "agent-sign-in",
    })
}
fn navigation(page: Page, glyph: ImageAsset) -> impl IntoElement {
    ui::action(move || routes::open(page))
        .accessibility_label(title(page))
        .child(
            ControlRow::default()
                .leading(Some(icon(glyph).into_element()))
                .content(Some(
                    text(move || title(page))
                        .text_size(crate::shared::ui::font_size)
                        .into_element(),
                ))
                .trailing(Some(icon(ui::NEXT).into_element()))
                .render(),
        )
}
fn overview() -> impl IntoElement {
    let search = signal(String::new());
    let matches = move |page| {
        title(page)
            .to_lowercase()
            .contains(&search.get().to_lowercase())
    };
    scroll().flex_1().child(el! { .view { p_4; gap_4 } [
        $(ui::field(tr!("search-settings"),move ||search.get(),move |v|search.set(v.into()))),
        $(ui::section(tr!("frontend"),view()
            .child(when(move ||matches(Page::Appearance),||navigation(Page::Appearance,ui::PALETTE)))
            .child(when(move ||matches(Page::ChatPreferences),||navigation(Page::ChatPreferences,ui::CHAT)))
            .child(when(move ||matches(Page::Connections),||navigation(Page::Connections,ui::PLUG)))
            .child(when(move ||matches(Page::Terminal),||navigation(Page::Terminal,ui::TERMINAL))))),
        $(ui::section(tr!("backend-settings"),view().child(backend::picker())
            .child(when(move ||matches(Page::Policy),||navigation(Page::Policy,ui::SETTINGS)))
            .child(when(move ||matches(Page::Agents),||navigation(Page::Agents,ui::BOT))))),
    ] })
}
fn about() -> impl IntoElement {
    scroll().flex_1().child(
        view()
            .p_4()
            .gap_4()
            .child(text("BAS").text_2xl().font_weight(FontWeight::SEMIBOLD))
            .child(ui::setting(tr!("version"), text(env!("CARGO_PKG_VERSION"))))
            .child(ui::setting(tr!("ui-framework"), text("Rust · NSH")))
            .child(ui::labeled_action(
                ui::EXTERNAL,
                tr!("framework-docs"),
                || {
                    spawn(async {
                        open_url("https://nsh.dev").await;
                    });
                },
            )),
    )
}
