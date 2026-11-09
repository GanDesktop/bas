use super::{current, fail, state};
use crate::shared::api;
use bas_client::service::pb;
use nsh_core::{prelude::*, storage::Storage};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const KEY: &str = "bas.navigation";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Marks {
    pinned: bool,
    bookmarked: bool,
    turns: BTreeSet<u64>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Saved(BTreeMap<String, BTreeMap<String, Marks>>);

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum Filter {
    #[default]
    Recent,
    Bookmarked,
    Archived,
}
pub(crate) fn filter() -> Signal<Filter> {
    global::<Filter>()
}
pub(super) fn initialize() {
    if let Some(saved) = Storage::get_item(KEY).and_then(|s| serde_json::from_str(&s).ok()) {
        global::<Saved>().set(saved);
    }
}
fn marks(session: &str) -> Marks {
    let endpoint = state().with(|s| s.endpoint.clone());
    global::<Saved>().with(|s| {
        s.0.get(&endpoint)
            .and_then(|s| s.get(session))
            .cloned()
            .unwrap_or_default()
    })
}
fn change(session: &str, edit: impl FnOnce(&mut Marks)) {
    let endpoint = state().with(|s| s.endpoint.clone());
    global::<Saved>().update(|s| {
        edit(
            s.0.entry(endpoint)
                .or_default()
                .entry(session.into())
                .or_default(),
        )
    });
    let json =
        serde_json::to_string(&global::<Saved>().get_untracked()).expect("navigation serializes");
    if let Err(error) = Storage::set_item(KEY, json) {
        fail(error.to_string());
    }
}
pub(crate) fn pinned(session: &str) -> bool {
    marks(session).pinned
}
pub(crate) fn bookmarked(session: &str) -> bool {
    marks(session).bookmarked
}
pub(crate) fn turn_bookmarked(session: &str, cursor: u64) -> bool {
    marks(session).turns.contains(&cursor)
}
pub(crate) fn toggle_pin(session: &str) {
    change(session, |m| m.pinned = !m.pinned);
}
pub(crate) fn toggle_bookmark(session: &str) {
    change(session, |m| m.bookmarked = !m.bookmarked);
}
pub(crate) fn toggle_turn(session: &str, cursor: u64) {
    change(session, |m| {
        if !m.turns.remove(&cursor) {
            m.turns.insert(cursor);
        }
    });
}
pub(crate) fn load_overview() {
    let s = state().get_untracked();
    let Some(session) = s.session else {
        return;
    };
    api::run(
        move |mut client| async move {
            Ok(client
                .load_history_overview(pb::LoadHistoryOverviewRequest { session })
                .await?
                .into_inner()
                .entries)
        },
        move |result| {
            if !current(s.generation, s.selection) {
                return;
            }
            match result {
                Ok(entries) => state().update(|s| s.overview = entries),
                Err(error) => fail(error.to_string()),
            }
        },
    );
}
