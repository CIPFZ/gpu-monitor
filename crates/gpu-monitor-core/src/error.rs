//! Stable sampling errors shared by every client.

use thiserror::Error;

/// Stable error categories shared by JSON, CLI and GUI clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    NotSupported,
    PermissionDenied,
    DeviceLost,
    Uninitialized,
    NoDevices,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Error)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[error("{message}")]
pub struct SampleError {
    pub kind: ErrorKind,
    pub message: String,
}

impl From<nvml_wrapper::error::NvmlError> for SampleError {
    fn from(error: nvml_wrapper::error::NvmlError) -> Self {
        use nvml_wrapper::error::NvmlError;
        let kind = match &error {
            NvmlError::NotSupported
            | NvmlError::FunctionNotFound
            | NvmlError::FailedToLoadSymbol(_) => ErrorKind::NotSupported,
            NvmlError::NoPermission | NvmlError::OperatingSystem => ErrorKind::PermissionDenied,
            NvmlError::GpuLost | NvmlError::ResetRequired => ErrorKind::DeviceLost,
            NvmlError::Uninitialized
            | NvmlError::DriverNotLoaded
            | NvmlError::LibraryNotFound
            | NvmlError::LibloadingError(_)
            | NvmlError::LibRmVersionMismatch => ErrorKind::Uninitialized,
            _ => ErrorKind::Unknown,
        };
        Self {
            kind,
            message: error.to_string(),
        }
    }
}
