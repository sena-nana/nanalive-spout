//! Error type for NanaVTS Spout output.

use core::fmt;

/// Errors returned by NanaVTS Spout output operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpoutOutputError {
    /// The current target cannot run Spout output.
    UnsupportedPlatform,
    /// The requested backend was not compiled in or could not be initialized.
    BackendUnavailable,
    /// The requested surface format is not supported by this backend.
    SurfaceFormatUnsupported,
    /// A required graphics-device interop path is unavailable.
    DeviceInteropUnavailable,
    /// Publishing a frame failed.
    PublishFailed,
    /// Sender name contains an interior NUL byte.
    InvalidSenderName,
    /// Frame dimensions are zero or do not fit in memory.
    InvalidFrameDimensions {
        /// Requested width in pixels.
        width: u32,
        /// Requested height in pixels.
        height: u32,
    },
    /// The supplied CPU pixel buffer is too small for the frame.
    BufferTooSmall {
        /// Minimum number of bytes required.
        expected: usize,
        /// Number of bytes provided.
        got: usize,
    },
}

impl fmt::Display for SpoutOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => write!(f, "Spout output is only supported on Windows"),
            Self::BackendUnavailable => write!(f, "Spout backend is unavailable"),
            Self::SurfaceFormatUnsupported => write!(f, "surface format is unsupported"),
            Self::DeviceInteropUnavailable => write!(f, "graphics device interop is unavailable"),
            Self::PublishFailed => write!(f, "failed to publish Spout frame"),
            Self::InvalidSenderName => write!(f, "sender name contains an interior NUL byte"),
            Self::InvalidFrameDimensions { width, height } => {
                write!(f, "invalid Spout frame dimensions {width}x{height}")
            }
            Self::BufferTooSmall { expected, got } => {
                write!(
                    f,
                    "pixel buffer too small: need {expected} bytes, got {got}"
                )
            }
        }
    }
}

impl std::error::Error for SpoutOutputError {}

/// Convenience result alias for Spout output operations.
pub type Result<T> = core::result::Result<T, SpoutOutputError>;
