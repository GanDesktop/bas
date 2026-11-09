use nsh_core::prelude::*;

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum Page {
    #[default]
    Conversation,
    Settings,
    Appearance,
    ChatPreferences,
    Connections,
    Terminal,
    About,
    Agents,
    AgentDetail,
    Authentication,
    Catalog,
    AgentEditor,
    Policy,
}
impl Page {
    pub fn parent(self) -> Self {
        match self {
            Self::Authentication => global::<AuthenticationReturn>().get_untracked().0,
            Self::Conversation | Self::Settings | Self::About => Self::Conversation,
            Self::Catalog | Self::AgentDetail | Self::AgentEditor => Self::Agents,
            _ => Self::Settings,
        }
    }
}
pub(crate) fn page() -> Signal<Page> {
    global::<Page>()
}
pub(crate) fn path() -> Vec<Page> {
    let mut current = page().get();
    let mut pages = vec![];
    while current != Page::Conversation {
        pages.push(current);
        current = current.parent();
    }
    pages.reverse();
    pages
}
pub(crate) fn open(next: Page) {
    dismiss_keyboard();
    page().set(next);
}
pub(crate) fn back() {
    open(page().get_untracked().parent());
}
pub(crate) fn agents() {
    open(Page::Agents);
}
pub(crate) fn catalog() {
    open(Page::Catalog);
}
pub(crate) fn close() {
    open(Page::Conversation);
}
#[derive(Clone, Default)]
struct Launch(Option<String>);
pub(crate) fn chat(agent: String) {
    global::<Launch>().set(Launch(Some(agent)));
    close();
}
pub(crate) fn take_launch() -> Option<String> {
    let agent = global::<Launch>().get_untracked().0;
    global::<Launch>().set(Launch(None));
    agent
}

#[derive(Clone, Default)]
struct AuthenticationReturn(Page);

pub(crate) fn authentication() {
    let origin = page().get_untracked();
    if origin != Page::Authentication {
        global::<AuthenticationReturn>().set(AuthenticationReturn(origin));
    }
    open(Page::Authentication);
}
