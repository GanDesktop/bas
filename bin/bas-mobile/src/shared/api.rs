use bas_client::service::Client;
use nsh_core::prelude::*;
use std::future::Future;

#[derive(Clone, Default)]
struct Connection {
    client: Option<Client>,
    backend: String,
    generation: u64,
    owner: Option<Owner>,
}

pub(crate) fn initialize() {
    global::<Connection>().update(|s| s.owner = Some(Owner::current()));
}

pub(crate) fn replace(client: Option<Client>, backend: String) {
    global::<Connection>().update(|s| {
        s.generation += 1;
        s.client = client;
        s.backend = backend;
    });
}

pub(crate) fn run<T, F, W>(work: W, done: impl FnOnce(anyhow::Result<T>) + 'static)
where
    T: Send + 'static,
    F: Future<Output = anyhow::Result<T>> + Send + 'static,
    W: FnOnce(Client) -> F + Send + 'static,
{
    let connection = global::<Connection>().get_untracked();
    let Some(client) = connection.client else {
        done(Err(anyhow::anyhow!(crate::shared::i18n::tr!("offline"))));
        return;
    };
    connection.owner.expect("app owner").run(|| {
        spawn(async move {
            let result = background(async move { work(client).await })
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))
                .and_then(|v| v);
            if global::<Connection>().get_untracked().generation == connection.generation {
                done(result);
            }
        })
    });
}

#[derive(Clone)]
pub(crate) struct Context {
    pub client: Client,
    pub backend: String,
    generation: u64,
}
impl Context {
    pub fn current(&self) -> bool {
        global::<Connection>().get_untracked().generation == self.generation
    }
}
pub(crate) fn context() -> Option<Context> {
    let connection = global::<Connection>().get_untracked();
    Some(Context {
        client: connection.client?,
        backend: connection.backend,
        generation: connection.generation,
    })
}
pub(crate) fn spawn_task(future: impl Future<Output = ()> + 'static) {
    global::<Connection>()
        .get_untracked()
        .owner
        .expect("app owner")
        .run(|| spawn(future));
}
