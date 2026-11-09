use crate::shared::i18n::tr;
use bas_browser_bridge::{Request, Response};
use nsh_core::prelude::*;
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Default)]
pub(crate) struct State {
    pub pending: Vec<Request>,
    pub busy: bool,
    pub error: String,
    directory: Option<PathBuf>,
    epoch: u64,
}
pub(crate) fn state() -> Signal<State> {
    global::<State>()
}

pub(super) fn watch(directory: Option<PathBuf>) {
    if state().get_untracked().directory == directory {
        return;
    }
    let epoch = state().get_untracked().epoch + 1;
    state().set(State {
        directory: directory.clone(),
        epoch,
        ..Default::default()
    });
    let Some(directory) = directory else { return };
    super::task(async move {
        while state().get_untracked().epoch == epoch {
            let path = directory.clone();
            let result = background(async move { bas_browser_bridge::pending(&path) }).await;
            if state().get_untracked().epoch != epoch {
                return;
            }
            match result {
                Ok(Ok(pending)) => {
                    if state().get_untracked().pending != pending {
                        state().update(|s| s.pending = pending);
                    }
                }
                Ok(Err(error)) => state().update(|s| {
                    s.pending.clear();
                    s.error = error.to_string();
                }),
                Err(error) => state().update(|s| s.error = error.to_string()),
            }
            timer(Duration::from_millis(250)).await;
        }
    });
}

pub(crate) fn open(request: Request) {
    answer(request, true);
}
pub(crate) fn dismiss(request: Request) {
    answer(request, false);
}

fn answer(request: Request, open: bool) {
    let snapshot = state().get_untracked();
    if snapshot.busy || !snapshot.pending.contains(&request) {
        return;
    }
    let Some(directory) = snapshot.directory else {
        return;
    };
    state().update(|s| {
        s.busy = true;
        s.error.clear();
    });
    super::task(async move {
        // Re-check the request immediately before a user-triggered system action.
        let path = directory.clone();
        let expected = request.clone();
        let pending = background(async move {
            bas_browser_bridge::pending(&path).map(|requests| requests.contains(&expected))
        })
        .await;
        if state().get_untracked().epoch != snapshot.epoch {
            return;
        }
        if !matches!(pending, Ok(Ok(true))) {
            state().update(|s| {
                s.busy = false;
                s.pending.retain(|p| p.id != request.id);
            });
            return;
        }
        if open && !open_url(request.url.clone()).await {
            state().update(|s| {
                s.busy = false;
                s.error = tr!("browser-open-failed");
            });
            return;
        }
        let id = request.id;
        let result = background(async move {
            bas_browser_bridge::respond(
                &directory,
                &request,
                if open {
                    Response::Opened
                } else {
                    Response::Cancelled
                },
            )
        })
        .await;
        if state().get_untracked().epoch != snapshot.epoch {
            return;
        }
        state().update(|s| {
            s.busy = false;
            match result {
                Ok(Ok(())) => s.pending.retain(|p| p.id != id),
                Ok(Err(error)) => s.error = error.to_string(),
                Err(error) => s.error = error.to_string(),
            }
        });
    });
}

#[cfg(all(test, unix))]
mod tests;
