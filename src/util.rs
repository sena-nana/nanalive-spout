use crate::{Result, SpoutOutputError};
#[cfg(windows)]
use core::ffi::c_char;
use std::ffi::CString;

#[allow(dead_code)]
pub(crate) fn cstring(s: &str) -> Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| SpoutOutputError::InvalidSenderName)
}

#[allow(dead_code)]
pub(crate) fn packed_rgba_pitch(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 {
        return Err(SpoutOutputError::InvalidFrameDimensions { width, height });
    }
    (width as usize)
        .checked_mul(4)
        .ok_or(SpoutOutputError::InvalidFrameDimensions { width, height })
}

#[allow(dead_code)]
pub(crate) fn validate_cpu_frame(
    pixels_len: usize,
    width: u32,
    height: u32,
    pitch_bytes: Option<u32>,
) -> Result<u32> {
    let pitch = match pitch_bytes {
        Some(0) | None => packed_rgba_pitch(width, height)?,
        Some(pitch) => pitch as usize,
    };
    let min_row = packed_rgba_pitch(width, height)?;
    if pitch < min_row {
        return Err(SpoutOutputError::BufferTooSmall {
            expected: min_row,
            got: pitch,
        });
    }
    let expected = pitch
        .checked_mul(height as usize)
        .ok_or(SpoutOutputError::InvalidFrameDimensions { width, height })?;
    if pixels_len < expected {
        return Err(SpoutOutputError::BufferTooSmall {
            expected,
            got: pixels_len,
        });
    }
    u32::try_from(pitch).map_err(|_| SpoutOutputError::InvalidFrameDimensions { width, height })
}

#[cfg(windows)]
#[allow(dead_code)]
pub(crate) fn read_cstr_buf<F>(fill: F) -> String
where
    F: FnOnce(*mut c_char, i32) -> i32,
{
    let mut buf = [0 as c_char; 256];
    let n = fill(buf.as_mut_ptr(), buf.len() as i32);
    if n <= 0 {
        return String::new();
    }
    let n = (n as usize).min(buf.len());
    let bytes = unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };
    String::from_utf8_lossy(bytes).into_owned()
}
