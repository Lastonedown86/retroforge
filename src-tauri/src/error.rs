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
    DeviceGone,
    #[error("memory write failed: {0}")]
    MemoryWriteFailed(String),
    #[error("memory read failed: {0}")]
    MemoryReadFailed(String),
    #[error("FEL exec failed: {0}")]
    ExecFailed(String),
    #[error("boot blob unavailable: {0}")]
    BlobMissing(String),
    #[error("memboot timed out waiting for device to leave FEL")]
    MembootTimeout,
    #[error("failed to fetch hakchi hmod: {0}")]
    HmodFetchFailed(String),
    #[error("failed to extract from hmod: {0}")]
    HmodExtractFailed(String),
    #[error("device shell unreachable over the network")]
    ShellNotFound,
    #[error("SSH error: {0}")]
    SshError(String),
    #[error("ramdisk error: {0}")]
    RamdiskError(String),
    #[error("invalid boot-screen PNG: {0}")]
    BootScreenInvalidPng(String),
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
