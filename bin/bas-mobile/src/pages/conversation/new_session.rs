use crate::{
    features::connection::{self as model, state},
    shared::{
        i18n::tr,
        ui::{self, ControlRow, icon},
    },
};
use gds_uim::component;
use nsh_base::theme;
use nsh_core::prelude::*;

#[component]
#[derive(Default)]
pub(super) struct NewSession {
    #[attr]
    trigger: Option<AnyElement>,
    #[attr]
    align: Align,
}

impl NewSession {
    pub(super) fn render(self) -> impl IntoElement {
        let open = signal(false);
        let trigger = self.trigger.unwrap_or_else(|| {
            view()
                .label(tr!("new-session"))
                .child(
                    ControlRow::default()
                        .leading(Some(icon(ui::COMPOSE).into_element()))
                        .content(Some(
                            text(tr!("new-session"))
                                .text_size(ui::font_size)
                                .into_element(),
                        ))
                        .render(),
                )
                .into_element()
        });
        ui::popup(open).align(self.align).trigger(trigger).child(
            view()
                .child(
                    text(tr!("new-session-agent"))
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(|| theme().muted_foreground),
                )
                .child(each(
                    || state().with(|s| s.agents.clone()),
                    |a| a.id.clone(),
                    move |agent, _| {
                        let id = agent.get().id;
                        ui::action(move || {
                            open.set(false);
                            model::choose_agent(&Some(id.clone().into()));
                            model::create_session();
                        })
                        .accessibility_label(move || {
                            Some(SharedString::from(agent.with(|a| a.id.clone())))
                        })
                        .child(
                            ControlRow::default()
                                .leading(Some(
                                    crate::widgets::agent::icon(move || {
                                        agent.with(|a| a.id.clone())
                                    })
                                    .into_element(),
                                ))
                                .content(Some(
                                    text(move || agent.with(|a| a.id.clone()))
                                        .text_size(ui::font_size)
                                        .into_element(),
                                ))
                                .render(),
                        )
                        .disabled(|| {
                            state().with(|s| !s.connected || s.workspace.is_none() || s.loading)
                        })
                    },
                )),
        )
    }
}
