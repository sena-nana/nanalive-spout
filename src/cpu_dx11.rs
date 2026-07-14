#[cfg(windows)]
use crate::util::cstring;
use crate::util::validate_cpu_frame;
use crate::{
    Result, SpoutBackendKind, SpoutFormat, SpoutFrameRef, SpoutOutputError, SpoutSenderBackend,
    SpoutStatus,
};

#[cfg(windows)]
use core::ptr;

/// CPU pixel Spout sender using the DirectX 11 backend.
pub struct CpuDx11Sender {
    #[cfg(windows)]
    raw: *mut NANALIVE_spout_sys::spout_dx_t,
    released: bool,
    width: Option<u32>,
    height: Option<u32>,
    format: SpoutFormat,
    last_error: Option<String>,
}

impl CpuDx11Sender {
    /// Create a CPU DX11 Spout sender.
    pub fn new(name: &str) -> Result<Self> {
        #[cfg(not(windows))]
        {
            let _ = name;
            Err(SpoutOutputError::UnsupportedPlatform)
        }
        #[cfg(windows)]
        {
            let cname = cstring(name)?;
            unsafe {
                let raw = NANALIVE_spout_sys::spout_dx_create();
                if raw.is_null() {
                    return Err(SpoutOutputError::BackendUnavailable);
                }
                if NANALIVE_spout_sys::spout_dx_open_directx11(raw, ptr::null_mut()) == 0
                    || NANALIVE_spout_sys::spout_dx_get_device(raw).is_null()
                {
                    NANALIVE_spout_sys::spout_dx_destroy(raw);
                    return Err(SpoutOutputError::BackendUnavailable);
                }
                if NANALIVE_spout_sys::spout_dx_set_sender_name(raw, cname.as_ptr()) == 0 {
                    NANALIVE_spout_sys::spout_dx_destroy(raw);
                    return Err(SpoutOutputError::BackendUnavailable);
                }
                NANALIVE_spout_sys::spout_dx_set_sender_format(
                    raw,
                    SpoutFormat::default().dxgi_format(),
                );
                Ok(Self {
                    raw,
                    released: false,
                    width: None,
                    height: None,
                    format: SpoutFormat::default(),
                    last_error: None,
                })
            }
        }
    }

    fn set_error(&mut self, error: SpoutOutputError) -> SpoutOutputError {
        self.last_error = Some(error.to_string());
        error
    }
}

impl SpoutSenderBackend for CpuDx11Sender {
    fn status(&self) -> SpoutStatus {
        #[cfg(not(windows))]
        {
            SpoutStatus::unavailable(
                SpoutBackendKind::CpuDx11,
                self.last_error
                    .clone()
                    .unwrap_or_else(|| SpoutOutputError::UnsupportedPlatform.to_string()),
            )
        }
        #[cfg(windows)]
        unsafe {
            let active =
                !self.released && NANALIVE_spout_sys::spout_dx_is_initialized(self.raw) != 0;
            SpoutStatus {
                available: true,
                enabled: !self.released,
                active,
                backend: Some(SpoutBackendKind::CpuDx11),
                width: if active {
                    Some(NANALIVE_spout_sys::spout_dx_get_width(self.raw))
                } else {
                    self.width
                },
                height: if active {
                    Some(NANALIVE_spout_sys::spout_dx_get_height(self.raw))
                } else {
                    self.height
                },
                fps: if active {
                    Some(NANALIVE_spout_sys::spout_dx_get_fps(self.raw))
                } else {
                    None
                },
                frame: if active {
                    Some(NANALIVE_spout_sys::spout_dx_get_frame(self.raw) as i64)
                } else {
                    None
                },
                error: self.last_error.clone(),
            }
        }
    }

    fn resize_or_recreate(&mut self, width: u32, height: u32, format: SpoutFormat) -> Result<()> {
        if !format.is_supported() {
            return Err(self.set_error(SpoutOutputError::SurfaceFormatUnsupported));
        }
        if width == 0 || height == 0 {
            return Err(self.set_error(SpoutOutputError::InvalidFrameDimensions { width, height }));
        }
        self.width = Some(width);
        self.height = Some(height);
        self.format = format;

        #[cfg(windows)]
        if !self.released {
            unsafe {
                NANALIVE_spout_sys::spout_dx_set_sender_format(self.raw, format.dxgi_format())
            };
        }

        Ok(())
    }

    fn publish(&mut self, frame: SpoutFrameRef<'_>) -> Result<()> {
        if self.released {
            return Err(self.set_error(SpoutOutputError::BackendUnavailable));
        }
        let SpoutFrameRef::CpuPixels {
            pixels,
            width,
            height,
            pitch_bytes,
        } = frame
        else {
            return Err(self.set_error(SpoutOutputError::PublishFailed));
        };
        let pitch = match validate_cpu_frame(pixels.len(), width, height, pitch_bytes) {
            Ok(pitch) => pitch,
            Err(err) => return Err(self.set_error(err)),
        };
        if self.width != Some(width) || self.height != Some(height) {
            self.resize_or_recreate(width, height, self.format)?;
        }

        #[cfg(not(windows))]
        {
            let _ = (pixels, pitch);
            Err(self.set_error(SpoutOutputError::UnsupportedPlatform))
        }
        #[cfg(windows)]
        unsafe {
            let ok = NANALIVE_spout_sys::spout_dx_send_image(
                self.raw,
                pixels.as_ptr(),
                width,
                height,
                pitch,
            );
            if ok == 0 {
                return Err(self.set_error(SpoutOutputError::PublishFailed));
            }
            self.last_error = None;
            Ok(())
        }
    }

    fn release(&mut self) {
        if self.released {
            return;
        }
        #[cfg(windows)]
        unsafe {
            NANALIVE_spout_sys::spout_dx_release_sender(self.raw);
        }
        self.released = true;
    }
}

impl Drop for CpuDx11Sender {
    fn drop(&mut self) {
        self.release();
        #[cfg(windows)]
        unsafe {
            NANALIVE_spout_sys::spout_dx_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for CpuDx11Sender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CpuDx11Sender")
            .field("status", &self.status())
            .finish()
    }
}
