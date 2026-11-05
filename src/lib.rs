//! Bas — a Basic Agent Service.
//!
//! Shared types and service support for `bas`, `bast`, and future frontends:
//!
//! * [`model`] — daemon lifecycle and replay envelopes.
//! * [`service`] — gRPC envelopes, ACP payloads, and transport primitives.
//! * [`config`] — shared settings, validation, and section-scoped persistence.
//!
//! Backend drivers, session management, runtime configuration, and the
//! gRPC service implementation belong to the `bas` binary.

pub mod config;
pub mod service;
pub use bas_protocol::{acp, acp_v2, authentication, management, model};
