use serde::Serialize;
use thiserror::Error;

/// Errors crossing the Tauri boundary. Serialized to a tagged object for the frontend.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum RfError {
    #[error("failed to claim USB interface: {0}")]
    UsbClaimFailed(String),
    #[error("FEL protocol error: {0}")]
    FelProtocolError(String),
    #[error("device disconnected during operation")]
    #[allow(dead_code)] // reserved for slice 2 (memboot/NAND); not yet constructed
    DeviceGone,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_tagged_object() {
        let e = RfError::UsbClaimFailed("busy".into());
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"kind":"UsbClaimFailed","message":"busy"}"#);
    }
}
