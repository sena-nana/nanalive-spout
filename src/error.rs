//! Error type for the safe Spout2 API.

use core::fmt;

/// Errors returned by Spout2 operations.
///
/// The underlying Spout SDK reports failure only via `false` return values and
/// null pointers — there is no error code or message — so these variants
/// capture *which* operation failed plus the cases we can detect structurally
/// (buffer sizing, interior NULs, null handles).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpoutError {
    /// A Spout operation returned `false`. The string names the operation.
    OperationFailed(&'static str),
    /// An operation required an initialized sender/receiver/device that was not ready.
    NotInitialized,
    /// The SDK returned a null handle/pointer where a value was expected.
    NullHandle(&'static str),
    /// A supplied pixel buffer was too small for `width * height * channels`.
    BufferSize {
        /// Minimum number of bytes required.
        expected: usize,
        /// Number of bytes actually provided.
        got: usize,
    },
    /// The requested image dimensions overflowed `usize` when calculating buffer size.
    DimensionOverflow {
        /// Requested width in pixels.
        width: u32,
        /// Requested height in pixels.
        height: u32,
        /// Bytes required for each row (saturated to `usize::MAX` if the per-row
        /// size itself overflowed).
        bytes_per_row: usize,
    },
    /// A name contained an interior NUL byte and could not be passed to C.
    InvalidName,
    /// Failed to create a graphics device or context. The string names the resource.
    InitFailed(&'static str),
}

impl fmt::Display for SpoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpoutError::OperationFailed(op) => write!(f, "Spout operation `{op}` failed"),
            SpoutError::NotInitialized => write!(f, "Spout object is not initialized"),
            SpoutError::NullHandle(what) => write!(f, "Spout returned a null {what}"),
            SpoutError::BufferSize { expected, got } => write!(
                f,
                "pixel buffer too small: need at least {expected} bytes, got {got}"
            ),
            SpoutError::DimensionOverflow {
                width,
                height,
                bytes_per_row,
            } => write!(
                f,
                "image dimensions {width}x{height} at {bytes_per_row} bytes per row overflow usize"
            ),
            SpoutError::InvalidName => write!(f, "name contained an interior NUL byte"),
            SpoutError::InitFailed(what) => write!(f, "failed to initialize {what}"),
        }
    }
}

impl std::error::Error for SpoutError {}

/// Convenience result alias for Spout operations.
pub type Result<T> = core::result::Result<T, SpoutError>;
