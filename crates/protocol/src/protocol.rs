//! Protobuf envelopes, native ACP payload conversions and history views.
mod convert;
mod defaults;
mod history;
mod observation;
pub use convert::*;
pub use defaults::*;
pub use history::*;

/// Header identifying a prepared ACP transport connection.
pub const ACP_CONNECTION_HEADER: &str = "bas-acp-connection";

pub mod pb {
    tonic::include_proto!("bas.v1");
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("bas_descriptor");
}

#[cfg(test)]
mod tests;
