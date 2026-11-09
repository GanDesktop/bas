use crate::{
    features::{agent_management, connection, session_policy},
    pages::{conversation, settings},
    shared::{api, preferences, routes},
};
use nsh_base::{nav_page, nav_stack, theme};
use nsh_core::platform::Lifecycle;
use nsh_core::prelude::*;

pub(crate) fn root() -> impl IntoElement {
    preferences::initialize();
    let language = computed(|| preferences::state().with(|p| p.language.clone()));
    provide_context(
        nsh_base::TextViewParts::new()
            .part(nsh_base::TextViewPart::Paragraph, |s| {
                s.text_size(|| px(preferences::state().with(|p| p.content_size)))
                    .line_height(|| {
                        px(preferences::state().with(|p| p.content_size * p.line_height))
                    })
            })
            .part(nsh_base::TextViewPart::Code, |s| {
                s.text_size(|| px(preferences::state().with(|p| p.code_size)))
                    .bg(|| theme().muted)
                    .rounded_sm()
            })
            .part(nsh_base::TextViewPart::CodeBlock, |s| {
                s.rounded_lg()
                    .border_1()
                    .border_color(|| theme().border)
                    .bg(|| theme().muted)
                    .min_w(px(0.0))
                    .overflow_hidden()
            })
            .part(nsh_base::TextViewPart::CodeBlockCode, |s| {
                s.p_3()
                    .text_size(|| px(preferences::state().with(|p| p.code_size)))
                    .line_height(|| px(preferences::state().with(|p| p.code_size * p.line_height)))
            }),
    );
    api::initialize();
    connection::initialize();
    crate::features::local_service::initialize();
    effect(|| {
        let local = crate::features::local_service::status().get();
        if local.available
            && matches!(local.phase.as_str(), "stopped" | "stopping" | "error")
            && connection::state().with(|s| s.backend.endpoint() == Some(local.endpoint.as_str()))
        {
            connection::disconnect();
        }
    });
    let backend = stored(None::<Option<String>>);
    effect(move || {
        let current = connection::state().with(|s| s.backend.endpoint().map(str::to_owned));
        if backend.get().as_ref() != Some(&current) {
            backend.replace(Some(current));
            if routes::page().get_untracked() == routes::Page::Authentication {
                agent_management::auth::cancel();
                routes::back();
            }
            match routes::page().get_untracked() {
                routes::Page::Policy => session_policy::load(),
                routes::Page::Agents | routes::Page::Catalog | routes::Page::AgentDetail => {
                    agent_management::load(false)
                }
                _ => {}
            }
        }
    });
    effect(|| {
        let page = routes::page().get();
        if page == routes::Page::Conversation
            && connection::state().get_untracked().backend.connected()
        {
            connection::management::reload();
            if let Some(agent) = routes::take_launch() {
                connection::choose_agent(&Some(agent.into()));
                connection::create_session();
            }
        }
    });
    let required = stored(None);
    effect(move || {
        let next = connection::state().with(|s| s.login_request.clone());
        if required.get() != next {
            required.replace(next.clone());
            if let Some(next) = next {
                agent_management::auth::open_agent(next.target.agent().to_owned());
            }
        }
    });
    let authenticating = stored(false);
    effect(move || {
        let open = routes::page().get() == routes::Page::Authentication;
        if authenticating.replace(open) && !open {
            agent_management::auth::cancel();
        }
    });
    let last_lifecycle = stored(lifecycle());
    effect(move || {
        let current = lifecycle();
        if last_lifecycle.replace(current) != Lifecycle::Active
            && current == Lifecycle::Active
            && routes::page().get_untracked() == routes::Page::Authentication
            && agent_management::auth::state().get_untracked().running()
        {
            agent_management::auth::recheck();
        }
    });
    effect(|| {
        if routes::page().get() == routes::Page::Authentication
            && agent_management::auth::connected()
            && routes::Page::Authentication.parent() == routes::Page::Conversation
        {
            let agent = agent_management::auth::state().get_untracked().key.agent;
            routes::back();
            connection::authentication_completed(&agent);
        }
    });
    let navigation = nav_stack()
        .flex_1()
        .font_family(|| {
            theme()
                .font_sans
                .clone()
                .unwrap_or_else(|| "sans-serif".into())
        })
        .bg(|| theme().sidebar)
        .text_color(|| theme().foreground)
        .child(nav_page().header_visible(false).child(dynamic(move || {
            let _ = language.get();
            conversation::render()
        })))
        .child(each(
            routes::path,
            |page| *page,
            move |page, _| {
                let page = page.get_untracked();
                nav_page()
                    .header_visible(false)
                    .animation(|| {
                        if preferences::state().with(|p| p.animations) {
                            ScreenAnimation::Default
                        } else {
                            ScreenAnimation::None
                        }
                    })
                    .on_dismiss(move || {
                        if routes::path().contains(&page) {
                            routes::open(page.parent());
                        }
                    })
                    .child(
                        view()
                            .flex_1()
                            .py_safe_area()
                            .mb_keyboard()
                            .bg(|| theme().background)
                            .text_color(|| theme().foreground)
                            .child(dynamic(move || {
                                let _ = language.get();
                                if page == routes::Page::Authentication {
                                    crate::pages::authentication::render().into_element()
                                } else {
                                    settings::render(page).into_element()
                                }
                            })),
                    )
            },
        ));
    view()
        .flex_1()
        .child(navigation)
        .child(crate::widgets::browser_requests::overlay())
}
