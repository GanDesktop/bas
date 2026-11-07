//! Real daemon and CLI processes; each scenario retains its own logs and database.

#[path = "e2e/configuration.rs"]
mod configuration;
#[path = "e2e/gateway.rs"]
mod gateway;
#[path = "e2e/sessions.rs"]
mod sessions;
#[path = "e2e/support.rs"]
mod support;
#[path = "e2e/transport.rs"]
mod transport;
#[path = "e2e/workspaces.rs"]
mod workspaces;

#[cfg(unix)]
#[path = "e2e/recovery.rs"]
mod recovery;

#[path = "e2e/tls.rs"]
mod tls;
