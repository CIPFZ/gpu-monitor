//! Error types for GPU monitoring operations

use thiserror::Error;

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during GPU monitoring
#[derive(Error, Debug)]
pub enum Error {
    /// NVML library initialization failed
    #[error("Failed to initialize NVML: {0}")]
    NvmlInit(String),

    /// NVML operation failed
    #[error("NVML error: {0}")]
    Nvml(#[from] nvml_wrapper::error::NvmlError),

    /// No GPU devices found
    #[error("No NVIDIA GPU devices found")]
    NoDevices,

    /// Invalid device index
    #[error("Invalid GPU device index: {0}")]
    InvalidDevice(u32),

    /// Failed to get process information
    #[error("Failed to get process info: {0}")]
    ProcessInfo(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
