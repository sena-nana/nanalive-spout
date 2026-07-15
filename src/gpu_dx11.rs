//! DirectX 11 texture sender for the Windows NanaLive Link receiver.

#[cfg(windows)]
use crate::util::cstring;
use crate::{Result, SpoutFormat, SpoutOutputError, SpoutPublishStatus};
use core::ffi::c_void;
#[cfg(windows)]
use std::time::Instant;

/// Raw `ID3D11Device` COM pointer marker.
pub type ID3D11Device = c_void;
/// Raw `ID3D11DeviceContext` COM pointer marker.
pub type ID3D11DeviceContext = c_void;
/// Raw `ID3D11Texture2D` COM pointer marker.
pub type ID3D11Texture2D = c_void;

/// Per-frame DX11 texture publish policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDx11PublishOptions {
    /// Input texture width.
    pub width: u32,
    /// Input texture height.
    pub height: u32,
    /// Input texture format. Version 1 accepts only premultiplied BGRA8.
    pub format: SpoutFormat,
    /// Maximum time to wait for the Spout shared-texture access lock.
    pub access_timeout_ms: u32,
    /// Collect coarse CPU-side timing around access, copy, and flush.
    pub collect_timing: bool,
}

impl GpuDx11PublishOptions {
    /// Construct the default NanaLive Link publish policy for a frame size.
    pub const fn bgra8(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: SpoutFormat::B8G8R8A8_UNORM,
            access_timeout_ms: 1,
            collect_timing: false,
        }
    }

    fn validate(self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(SpoutOutputError::InvalidFrameDimensions {
                width: self.width,
                height: self.height,
            });
        }
        if self.format != SpoutFormat::B8G8R8A8_UNORM {
            return Err(SpoutOutputError::SurfaceFormatUnsupported);
        }
        Ok(())
    }
}

/// CPU-side timing for one direct DX11 texture publish.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpoutDx11Timing {
    /// Time spent waiting for shared-texture access.
    pub access_wait_us: u64,
    /// Time spent issuing the one GPU `CopyResource` operation.
    pub copy_us: u64,
    /// Time spent flushing the immediate context when required by Spout.
    pub flush_us: u64,
    /// End-to-end publish call duration.
    pub total_us: u64,
}

/// Result of publishing one existing DX11 texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDx11PublishReport {
    /// Publish outcome.
    pub status: SpoutPublishStatus,
    /// Sender frame counter after publishing, when known.
    pub frame: Option<i64>,
    /// Optional timing diagnostics.
    pub timing: Option<SpoutDx11Timing>,
}

/// Current direct DX11 sender state and lifetime counters.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuDx11Status {
    /// Whether this backend can run on the current target and build.
    pub available: bool,
    /// Whether the sender has not been released.
    pub enabled: bool,
    /// Whether Spout has initialized a sender surface.
    pub active: bool,
    /// Current sender width.
    pub width: Option<u32>,
    /// Current sender height.
    pub height: Option<u32>,
    /// Measured sender frame rate.
    pub fps: Option<f64>,
    /// Current sender frame counter.
    pub frame: Option<i64>,
    /// Successfully submitted frames.
    pub sent_count: u64,
    /// Frames skipped because access was not acquired before the deadline.
    pub skipped_access_timeout_count: u64,
    /// Failed publish attempts.
    pub failed_count: u64,
    /// Number of accepted size changes that recreate the shared surface.
    pub surface_recreation_count: u64,
    /// The fixed pixel format contract.
    pub format: SpoutFormat,
    /// Whether the output carries alpha.
    pub has_alpha: bool,
    /// The fixed alpha mode contract.
    pub alpha_mode: &'static str,
    /// Last publish error, if any.
    pub error: Option<String>,
}

/// Publishes an existing premultiplied BGRA D3D11 texture without CPU readback.
pub struct GpuDx11TextureSender {
    #[cfg(windows)]
    raw: *mut nanalive_spout_sys::spout_dx11_gpu_t,
    released: bool,
    width: Option<u32>,
    height: Option<u32>,
    sent_count: u64,
    skipped_access_timeout_count: u64,
    failed_count: u64,
    surface_recreation_count: u64,
    last_error: Option<String>,
}

impl GpuDx11TextureSender {
    /// Create a sender on the caller's existing D3D11 device and immediate context.
    ///
    /// # Safety
    ///
    /// `device` and `context` must be valid matching COM interfaces and must outlive
    /// the sender. The native layer validates that the context belongs to `device`.
    pub unsafe fn new(
        name: &str,
        device: *mut ID3D11Device,
        context: *mut ID3D11DeviceContext,
    ) -> Result<Self> {
        if device.is_null() || context.is_null() {
            return Err(SpoutOutputError::DeviceInteropUnavailable);
        }

        #[cfg(not(windows))]
        {
            let _ = name;
            Err(SpoutOutputError::UnsupportedPlatform)
        }
        #[cfg(windows)]
        unsafe {
            let cname = cstring(name)?;
            let raw = nanalive_spout_sys::spout_dx11_gpu_create();
            if raw.is_null() {
                return Err(SpoutOutputError::BackendUnavailable);
            }
            if nanalive_spout_sys::spout_dx11_gpu_open(raw, device, context) == 0
                || nanalive_spout_sys::spout_dx11_gpu_set_sender_name(raw, cname.as_ptr()) == 0
            {
                nanalive_spout_sys::spout_dx11_gpu_destroy(raw);
                return Err(SpoutOutputError::DeviceInteropUnavailable);
            }
            Ok(Self {
                raw,
                released: false,
                width: None,
                height: None,
                sent_count: 0,
                skipped_access_timeout_count: 0,
                failed_count: 0,
                surface_recreation_count: 0,
                last_error: None,
            })
        }
    }

    /// Publish one existing D3D11 texture.
    ///
    /// The native boundary verifies same-device ownership, BGRA8 format, default
    /// usage, single-sample/single-mip geometry, and render/shader compatibility.
    ///
    /// # Safety
    ///
    /// `texture` must be a valid `ID3D11Texture2D*` and remain alive until this
    /// call returns. It must belong to the device passed to [`Self::new`].
    pub unsafe fn publish_texture(
        &mut self,
        texture: *mut ID3D11Texture2D,
        options: GpuDx11PublishOptions,
    ) -> Result<GpuDx11PublishReport> {
        options.validate()?;
        if texture.is_null() {
            return Err(SpoutOutputError::PublishFailed);
        }
        if self.released {
            return Ok(self.report(SpoutPublishStatus::BackendUnavailable, None, None));
        }

        #[cfg(not(windows))]
        {
            let _ = (texture, options);
            Ok(self.report(SpoutPublishStatus::BackendUnavailable, None, None))
        }
        #[cfg(windows)]
        unsafe {
            let total_start = options.collect_timing.then(Instant::now);
            let mut native = nanalive_spout_sys::spout_dx11_send_result_t::default();
            let native_ok = nanalive_spout_sys::spout_dx11_gpu_send_texture(
                self.raw,
                texture,
                options.width,
                options.height,
                options.format.dxgi_format(),
                options.access_timeout_ms,
                u32::from(options.collect_timing),
                &mut native,
            );
            let status = if native_ok == 0 {
                SpoutPublishStatus::Failed
            } else {
                match native.status {
                    1 => SpoutPublishStatus::Sent,
                    2 => SpoutPublishStatus::SkippedAccessTimeout,
                    _ => SpoutPublishStatus::Failed,
                }
            };
            if status == SpoutPublishStatus::Sent
                && (self.width != Some(options.width) || self.height != Some(options.height))
            {
                self.surface_recreation_count = self.surface_recreation_count.saturating_add(1);
                self.width = Some(options.width);
                self.height = Some(options.height);
            }
            let frame = (native.frame >= 0).then_some(native.frame as i64);
            let timing = total_start.map(|start| SpoutDx11Timing {
                access_wait_us: native.access_wait_us,
                copy_us: native.copy_us,
                flush_us: native.flush_us,
                total_us: elapsed_us(start),
            });
            Ok(self.report(status, frame, timing))
        }
    }

    /// Return sender status and monotonic lifetime counters.
    pub fn status(&self) -> GpuDx11Status {
        #[cfg(not(windows))]
        {
            GpuDx11Status {
                available: false,
                enabled: false,
                active: false,
                width: self.width,
                height: self.height,
                fps: None,
                frame: None,
                sent_count: self.sent_count,
                skipped_access_timeout_count: self.skipped_access_timeout_count,
                failed_count: self.failed_count,
                surface_recreation_count: self.surface_recreation_count,
                format: SpoutFormat::B8G8R8A8_UNORM,
                has_alpha: true,
                alpha_mode: "premultiplied",
                error: self
                    .last_error
                    .clone()
                    .or_else(|| Some(SpoutOutputError::UnsupportedPlatform.to_string())),
            }
        }
        #[cfg(windows)]
        unsafe {
            let active = !self.released
                && !self.raw.is_null()
                && nanalive_spout_sys::spout_dx11_gpu_is_initialized(self.raw) != 0;
            GpuDx11Status {
                available: true,
                enabled: !self.released,
                active,
                width: if active {
                    Some(nanalive_spout_sys::spout_dx11_gpu_get_width(self.raw))
                } else {
                    self.width
                },
                height: if active {
                    Some(nanalive_spout_sys::spout_dx11_gpu_get_height(self.raw))
                } else {
                    self.height
                },
                fps: active.then(|| nanalive_spout_sys::spout_dx11_gpu_get_fps(self.raw)),
                frame: active
                    .then(|| nanalive_spout_sys::spout_dx11_gpu_get_frame(self.raw) as i64),
                sent_count: self.sent_count,
                skipped_access_timeout_count: self.skipped_access_timeout_count,
                failed_count: self.failed_count,
                surface_recreation_count: self.surface_recreation_count,
                format: SpoutFormat::B8G8R8A8_UNORM,
                has_alpha: true,
                alpha_mode: "premultiplied",
                error: self.last_error.clone(),
            }
        }
    }

    /// Release sender resources. Calling this more than once is harmless.
    pub fn release(&mut self) {
        if self.released {
            return;
        }
        #[cfg(windows)]
        unsafe {
            if !self.raw.is_null() {
                nanalive_spout_sys::spout_dx11_gpu_release_sender(self.raw);
            }
        }
        self.released = true;
    }

    fn report(
        &mut self,
        status: SpoutPublishStatus,
        frame: Option<i64>,
        timing: Option<SpoutDx11Timing>,
    ) -> GpuDx11PublishReport {
        match status {
            SpoutPublishStatus::Sent => {
                self.sent_count = self.sent_count.saturating_add(1);
                self.last_error = None;
            }
            SpoutPublishStatus::SkippedAccessTimeout => {
                self.skipped_access_timeout_count =
                    self.skipped_access_timeout_count.saturating_add(1);
                self.last_error = None;
            }
            SpoutPublishStatus::BackendUnavailable => {
                self.last_error = Some(SpoutOutputError::BackendUnavailable.to_string());
            }
            SpoutPublishStatus::InvalidFrame | SpoutPublishStatus::Failed => {
                self.failed_count = self.failed_count.saturating_add(1);
                self.last_error = Some(SpoutOutputError::PublishFailed.to_string());
            }
        }
        GpuDx11PublishReport {
            status,
            frame,
            timing,
        }
    }
}

impl Drop for GpuDx11TextureSender {
    fn drop(&mut self) {
        self.release();
        #[cfg(windows)]
        unsafe {
            if !self.raw.is_null() {
                nanalive_spout_sys::spout_dx11_gpu_destroy(self.raw);
            }
        }
    }
}

impl core::fmt::Debug for GpuDx11TextureSender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GpuDx11TextureSender")
            .field("status", &self.status())
            .finish()
    }
}

#[cfg(windows)]
fn elapsed_us(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_reject_zero_dimensions() {
        assert_eq!(
            GpuDx11PublishOptions::bgra8(0, 1080).validate(),
            Err(SpoutOutputError::InvalidFrameDimensions {
                width: 0,
                height: 1080
            })
        );
    }

    #[test]
    fn options_reject_non_bgra_format() {
        let options = GpuDx11PublishOptions {
            format: SpoutFormat::R8G8B8A8_UNORM,
            ..GpuDx11PublishOptions::bgra8(1920, 1080)
        };
        assert_eq!(
            options.validate(),
            Err(SpoutOutputError::SurfaceFormatUnsupported)
        );
    }

    #[test]
    fn non_windows_constructor_rejects_unavailable_backend() {
        #[cfg(not(windows))]
        unsafe {
            let result = GpuDx11TextureSender::new(
                "NanaLive Link",
                std::ptr::dangling_mut::<ID3D11Device>(),
                std::ptr::dangling_mut::<ID3D11DeviceContext>(),
            );
            assert!(matches!(result, Err(SpoutOutputError::UnsupportedPlatform)));
        }
    }
}
