//! Domain types shared by the daemon, the management CLI, and every client.
//!
//! Daemon lifecycle and replay envelopes wrap the shared ACP schema.
//! Content, tools and stop reasons are re-exported from [`crate::acp`].

mod approval;
mod content;
mod event;
pub mod history;
mod ids;
mod role;
mod session;
mod subagent;
mod submission;
mod time;
mod workspace;

pub use approval::*;
pub use content::*;
pub use event::*;
pub use ids::*;
pub use role::*;
pub use session::*;
pub use subagent::*;
pub use submission::*;
pub use time::*;
pub use workspace::*;
