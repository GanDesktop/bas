//! Service-side ACP adapters. Backend execution is supplied through the BAS trait.
pub use bas_protocol::{acp, authentication, model};
mod gateway;
pub use gateway::{Authentication, AuthenticationEvents, AuthenticationInput, Gateway};
