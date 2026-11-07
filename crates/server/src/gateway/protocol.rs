//! ACP envelope conversion and backend error mapping.
use super::*;
use serde_json::Value;

pub(super) fn protocol(error: impl std::fmt::Display) -> crate::acp::Error {
    crate::acp::Error::internal_error().data(error.to_string())
}
pub(super) fn status(error: Status) -> crate::acp::Error {
    bas_protocol::protocol::acp_error(&error).unwrap_or_else(|_| protocol(error.message()))
}
pub(super) fn payload(value: Value) -> pb::AcpPayload {
    pb::AcpPayload {
        json: serde_json::to_vec(&value).expect("protocol JSON"),
    }
}
pub(super) fn value(payload: pb::AcpPayload) -> Result<Value, crate::acp::Error> {
    serde_json::from_slice(&payload.json).map_err(protocol)
}
