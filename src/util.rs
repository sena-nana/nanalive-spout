//! Internal helpers for FFI string and buffer handling.

use crate::error::{Result, SpoutError};
use core::ffi::c_char;
use std::ffi::CString;

/// Convert a Rust string into a [`CString`], mapping an interior NUL byte to a
/// clean [`SpoutError::InvalidName`] instead of a panic.
#[allow(dead_code)] // used by the dx/gl backends (feature-gated)
pub(crate) fn cstring(s: &str) -> Result<CString> {
    cstring_bytes(s.as_bytes())
}

/// Convert arbitrary non-NUL bytes into a [`CString`] for Spout's ANSI names.
#[allow(dead_code)] // used by the dx/gl backends (feature-gated)
pub(crate) fn cstring_bytes(bytes: &[u8]) -> Result<CString> {
    CString::new(bytes.to_vec()).map_err(|_| SpoutError::InvalidName)
}

/// Calculate the byte length for a `width` x `height` image without wrapping.
#[allow(dead_code)] // used by the dx/gl backends (feature-gated)
pub(crate) fn image_byte_len(width: u32, height: u32, bytes_per_pixel: usize) -> Result<usize> {
    let bytes_per_row = (width as usize)
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| SpoutError::DimensionOverflow {
            width,
            height,
            // The per-row size itself overflowed; report the saturated value
            // rather than a misleading placeholder.
            bytes_per_row: (width as usize).saturating_mul(bytes_per_pixel),
        })?;
    image_byte_len_with_row_pitch(width, height, bytes_per_row)
}

/// Calculate the byte length for an image with an explicit row pitch.
#[allow(dead_code)] // used by the dx/gl backends (feature-gated)
pub(crate) fn image_byte_len_with_row_pitch(
    width: u32,
    height: u32,
    bytes_per_row: usize,
) -> Result<usize> {
    bytes_per_row
        .checked_mul(height as usize)
        .ok_or(SpoutError::DimensionOverflow {
            width,
            height,
            bytes_per_row,
        })
}

/// Read a C string written into a fixed-size buffer by a shim getter.
///
/// `fill` receives a writable buffer and its length and must write a
/// NUL-terminated string, returning the number of bytes written (the shim
/// convention). The bytes are decoded with [`String::from_utf8_lossy`] because
/// Spout's internal names are ANSI, not guaranteed UTF-8.
pub(crate) fn read_cstr_buf<F>(fill: F) -> String
where
    F: FnOnce(*mut c_char, i32) -> i32,
{
    String::from_utf8_lossy(&read_cstr_bytes(fill)).into_owned()
}

/// Read raw C string bytes written into a fixed-size buffer by a shim getter.
#[allow(dead_code)] // used by the dx/gl backends (feature-gated)
pub(crate) fn read_cstr_bytes<F>(fill: F) -> Vec<u8>
where
    F: FnOnce(*mut c_char, i32) -> i32,
{
    // Spout's internal sender-name buffers are char[256].
    let mut buf = [0 as c_char; 256];
    let n = fill(buf.as_mut_ptr(), buf.len() as i32);
    if n <= 0 {
        return Vec::new();
    }
    let n = (n as usize).min(buf.len());
    // SAFETY: `fill` wrote `n` bytes into `buf`; we read exactly those bytes.
    let bytes = unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };
    bytes.to_vec()
}

#[cfg(test)]
mod tests {
    use super::image_byte_len;
    use crate::SpoutError;

    #[test]
    fn image_byte_len_detects_overflow() {
        let err = image_byte_len(u32::MAX, u32::MAX, 4).expect_err("must reject overflow");
        assert!(matches!(
            err,
            SpoutError::DimensionOverflow {
                width: u32::MAX,
                height: u32::MAX,
                ..
            }
        ));
    }

    #[test]
    fn image_byte_len_returns_exact_size() {
        assert_eq!(image_byte_len(64, 32, 4), Ok(64 * 32 * 4));
    }
}
