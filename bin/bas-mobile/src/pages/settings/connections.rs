use crate::{
    features::connection::{self, profiles, state},
    shared::{
        i18n::tr,
        ui::{self, ControlRow},
    },
};
use gds_uim::{el, m};
use nsh_base::theme;
use nsh_core::prelude::*;

pub(super) fn render() -> impl IntoElement {
    let address = signal(state().get_untracked().endpoint);
    let name = signal(untracked(|| profiles::name(&address.get_untracked())));
    scroll().flex_1().child(
        view()
            .p_4()
            .child(local_service())
            .child(ui::section(tr!("saved-connections"), saved(address, name)))
            .child(ui::section(
                tr!("connection-details"),
                editor(address, name),
            )),
    )
}

fn local_service() -> impl IntoElement {
    use crate::features::local_service as service;
    let error = signal(String::new());
    let connect = move || {
        error.set(String::new());
        let endpoint = service::status().get_untracked().endpoint;
        spawn(async move {
            match service::prepare(endpoint.clone()).await {
                Ok(()) => {
                    profiles::save(&endpoint, &tr!("local-service"));
                    connection::edit_endpoint(&endpoint);
                    connection::connect();
                }
                Err(failure) => error.set(failure.to_string()),
            }
        });
    };
    when(
        || service::status().with(|s| s.available),
        move || {
            ui::section(
                tr!("local-service"),
                el! { .view { p_3; gap_3 } [
                    $(text(service::description)) { text_size: ui::font_size },
                    $(when(move || error.with(|s| !s.is_empty()), move || text(move || error.get()).text_size(ui::font_size))),
                    .view { flex_row; flex_wrap; justify_end; gap_2 } [
                        $(ui::labeled_action(ui::PLUG, tr!("local-service-start"), service::start)
                            .disabled(|| service::busy() || service::status().with(|s| s.phase == "running"))),
                        $(ui::labeled_action(ui::PLUG, tr!("local-service-stop"), service::stop)
                            .disabled(|| service::status().with(|s| s.phase != "running" && s.phase != "starting"))),
                        $(ui::labeled_action(ui::LOGIN, tr!("connect"), connect)
                            .disabled(|| service::status().with(|s| s.phase != "running"))),
                    ],
                ] },
            )
        },
    )
}

fn saved(address: Signal<String>, name: Signal<String>) -> impl IntoElement {
    each(
        || state().with(|s| s.addresses.clone()),
        |a| a.clone(),
        move |entry, _| {
            let select = move || {
                let value = entry.get_untracked();
                name.set(profiles::name(&value));
                address.set(value);
            };
            ui::action(select)
                .selected(move || address.with(|a| a.trim() == entry.get()))
                .selected_style(m! { bg: $move || theme().accent })
                .child(
                    ControlRow::default()
                        .leading(Some(
                            image(move || crate::widgets::backend::connection_icon(&entry.get()))
                                .size(px(18.0))
                                .tint_color(|| theme().foreground)
                                .into_element(),
                        ))
                        .content(Some(
                            text(move || profiles::name(&entry.get()))
                                .text_size(ui::font_size)
                                .truncate()
                                .into_element(),
                        ))
                        .trailing(Some(
                            when(
                                move || state().with(|s| s.endpoint == entry.get() && s.connected),
                                || {
                                    text(tr!("ready"))
                                        .text_xs()
                                        .text_color(|| theme().muted_foreground)
                                },
                            )
                            .into_element(),
                        ))
                        .render(),
                )
        },
    )
}

fn editor(address: Signal<String>, name: Signal<String>) -> impl IntoElement {
    let feedback = move || feedback(&address.get());
    el! { .view { p_3 } [
        .view { gap_1; mb_3 } [
            $(text(tr!("connection-name"))) { text_size: ui::font_size },
            $(ui::field(tr!("connection-name"), move || name.get(), move |v| name.set(v.into()))),
        ],
        .view { gap_1; mb_3 } [
            $(text(tr!("endpoint"))) { text_size: ui::font_size },
            $(ui::field(tr!("endpoint"), move || address.get(), move |v| address.set(v.into()))
                .keyboard_type(KeyboardType::Url)),
        ],
        $(actions(address, name)),
        $(when(move || !feedback().is_empty(), move || text(feedback)
            .mt_3().text_size(ui::font_size).text_color(|| theme().muted_foreground))),
    ] }
}

fn actions(address: Signal<String>, name: Signal<String>) -> impl IntoElement {
    let test = move || profiles::test(address.get_untracked().trim().into());
    let save = move || profiles::save(&address.get_untracked(), &name.get_untracked());
    let connect = move || {
        let selected = address.get_untracked();
        profiles::save(&selected, &name.get_untracked());
        connection::edit_endpoint(selected.trim());
        connection::connect();
    };
    let empty = move || address.with(|a| a.trim().is_empty());
    el! { .view { flex_row; flex_wrap; items_center; justify_end; gap_2 } [
        $(ui::labeled_action(ui::PLUG, tr!("test-connection"), test)
            .disabled(move || empty() || profiles::profiles().with(|p| p.testing))),
        $(ui::labeled_action(ui::SAVE, tr!("save"), save).disabled(empty)),
        $(ui::labeled_action(ui::LOGIN, tr!("connect"), connect)
            .bg(|| theme().accent)
            .disabled(move || empty() || state().with(|s| s.loading))),
    ] }
}

fn feedback(address: &str) -> String {
    let address = address.trim();
    let connection = state().with(|s| {
        if s.endpoint.trim() != address {
            String::new()
        } else if s.loading {
            tr!("loading")
        } else {
            s.error.clone()
        }
    });
    if !connection.is_empty() {
        return connection;
    }
    profiles::profiles().with(|p| {
        if p.message_address != address {
            String::new()
        } else if p.testing {
            tr!("loading")
        } else {
            p.message.clone()
        }
    })
}
