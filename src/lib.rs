//! NanaVTS-focused Spout sender output backend.
//!
//! The public API intentionally exposes only the controlled output surface used
//! by NanaVTS: CPU DirectX 11 sender output by default and an opt-in
//! experimental DirectX 12 GPU sender path.
#![warn(missing_docs)]

mod error;
mod util;

#[cfg(feature = "cpu-dx11")]
mod cpu_dx11;
#[cfg(feature = "gpu-dx12-experimental")]
mod gpu_dx12;

use core::ffi::c_void;

pub use error::{Result, SpoutOutputError};

#[cfg(feature = "cpu-dx11")]
pub use cpu_dx11::CpuDx11Sender;
#[cfg(feature = "gpu-dx12-experimental")]
pub use gpu_dx12::{
    GpuDx12ExperimentalSender, GpuDx12PublishOptions, ID3D12CommandQueue, ID3D12Device,
};

/// The Spout SDK version this crate is built against.
pub use nanavts_spout_sys::SPOUT_SDK_VERSION;

/// NanaVTS Spout output backend kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpoutBackendKind {
    /// CPU pixel upload through Spout's DirectX 11 sender path.
    CpuDx11,
    /// Experimental GPU texture output through Spout's D3D11On12 DX12 bridge.
    GpuDx12Experimental,
}

/// Supported shared-surface formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpoutFormat(u32);

impl SpoutFormat {
    /// `DXGI_FORMAT_B8G8R8A8_UNORM`, the default NanaVTS CPU output format.
    pub const B8G8R8A8_UNORM: Self = Self(87);
    /// `DXGI_FORMAT_R8G8B8A8_UNORM`.
    pub const R8G8B8A8_UNORM: Self = Self(28);
    /// `DXGI_FORMAT_R8G8B8A8_UNORM_SRGB`.
    pub const R8G8B8A8_UNORM_SRGB: Self = Self(29);

    /// Return the raw `DXGI_FORMAT` value.
    pub const fn dxgi_format(self) -> u32 {
        self.0
    }

    /// Build a supported format from a raw `DXGI_FORMAT` value.
    pub fn from_dxgi_format(dxgi_format: u32) -> Result<Self> {
        let format = Self(dxgi_format);
        if format.is_supported() {
            Ok(format)
        } else {
            Err(SpoutOutputError::SurfaceFormatUnsupported)
        }
    }

    pub(crate) const fn is_supported(self) -> bool {
        matches!(self.0, 87 | 28 | 29)
    }
}

impl Default for SpoutFormat {
    fn default() -> Self {
        Self::B8G8R8A8_UNORM
    }
}

/// Borrowed frame data to publish.
#[derive(Debug, Clone, Copy)]
pub enum SpoutFrameRef<'a> {
    /// CPU RGBA/BGRA 8-bit pixels. `pitch_bytes` may be `None` for tightly packed rows.
    CpuPixels {
        /// Borrowed pixel bytes.
        pixels: &'a [u8],
        /// Frame width in pixels.
        width: u32,
        /// Frame height in pixels.
        height: u32,
        /// Optional row pitch in bytes.
        pitch_bytes: Option<u32>,
    },
    /// Experimental D3D12 texture resource for GPU output.
    Dx12Resource {
        /// Raw `ID3D12Resource*`.
        resource: *mut c_void,
        /// `D3D12_RESOURCE_STATES` value describing the current resource state.
        initial_state: u32,
        /// `D3D12_RESOURCE_STATES` value to transition to after D3D11On12 release.
        final_state: u32,
    },
}

/// Per-frame publish outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpoutPublishStatus {
    /// The native backend submitted and signaled a new Spout frame.
    Sent,
    /// The Spout shared texture access mutex was not acquired before timeout.
    SkippedAccessTimeout,
    /// The backend is not available or has already been released.
    BackendUnavailable,
    /// The provided frame does not match this backend or is otherwise invalid.
    InvalidFrame,
    /// The backend attempted to publish and failed.
    Failed,
}

/// Optional coarse DX12 publish timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpoutDx12Timing {
    /// Time spent wrapping the D3D12 resource when the cache missed.
    pub wrap_us: u64,
    /// Time spent waiting for Spout shared texture access.
    pub access_wait_us: u64,
    /// Time spent submitting the D3D11 copy and D3D11On12 release.
    pub submit_us: u64,
    /// Time spent flushing the D3D11On12 context.
    pub flush_us: u64,
    /// End-to-end `publish_report` time.
    pub total_us: u64,
}

/// Per-frame publish result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpoutPublishReport {
    /// Publish outcome.
    pub status: SpoutPublishStatus,
    /// Spout frame counter after publishing, when known.
    pub frame: Option<i64>,
    /// Time spent waiting for Spout shared texture access, when reported.
    pub waited_us: Option<u64>,
    /// Optional DX12 timing diagnostics.
    pub timing: Option<SpoutDx12Timing>,
}

/// Current Spout output state.
#[derive(Debug, Clone, PartialEq)]
pub struct SpoutStatus {
    /// Whether this backend is available on the current platform/build.
    pub available: bool,
    /// Whether output is enabled by the owning application.
    pub enabled: bool,
    /// Whether a sender has published at least one frame.
    pub active: bool,
    /// Active backend kind, if available.
    pub backend: Option<SpoutBackendKind>,
    /// Current sender width.
    pub width: Option<u32>,
    /// Current sender height.
    pub height: Option<u32>,
    /// Measured Spout frame rate.
    pub fps: Option<f64>,
    /// Current Spout frame counter.
    pub frame: Option<i64>,
    /// Last backend error, if any.
    pub error: Option<String>,
}

impl SpoutStatus {
    /// Build an available but inactive status for a backend.
    pub fn available(backend: SpoutBackendKind) -> Self {
        Self {
            available: true,
            enabled: false,
            active: false,
            backend: Some(backend),
            width: None,
            height: None,
            fps: None,
            frame: None,
            error: None,
        }
    }

    /// Build an unavailable status for a backend.
    pub fn unavailable(backend: SpoutBackendKind, error: impl Into<String>) -> Self {
        Self {
            available: false,
            enabled: false,
            active: false,
            backend: Some(backend),
            width: None,
            height: None,
            fps: None,
            frame: None,
            error: Some(error.into()),
        }
    }
}

/// Common sender backend interface used by NanaVTS.
pub trait SpoutSenderBackend {
    /// Return the current backend status.
    fn status(&self) -> SpoutStatus;
    /// Resize or recreate the sender surface for the requested format.
    fn resize_or_recreate(&mut self, width: u32, height: u32, format: SpoutFormat) -> Result<()>;
    /// Publish one frame.
    fn publish(&mut self, frame: SpoutFrameRef<'_>) -> Result<()>;
    /// Publish one frame and return an explicit outcome report.
    fn publish_report(&mut self, frame: SpoutFrameRef<'_>) -> Result<SpoutPublishReport> {
        self.publish(frame)?;
        Ok(SpoutPublishReport {
            status: SpoutPublishStatus::Sent,
            frame: self.status().frame,
            waited_us: None,
            timing: None,
        })
    }
    /// Release the native sender resources.
    fn release(&mut self);
}

/// Returns the linked Spout SDK version string on Windows, or the vendored pin elsewhere.
pub fn sdk_version() -> String {
    #[cfg(all(windows, any(feature = "cpu-dx11", feature = "gpu-dx12-experimental")))]
    {
        util::read_cstr_buf(|buf, len| unsafe {
            nanavts_spout_sys::spout_get_sdk_version(buf, len)
        })
    }
    #[cfg(not(all(windows, any(feature = "cpu-dx11", feature = "gpu-dx12-experimental"))))]
    {
        SPOUT_SDK_VERSION.to_string()
    }
}

/// Report static backend availability for the current platform and compiled features.
pub fn backend_status(kind: SpoutBackendKind) -> SpoutStatus {
    match kind {
        SpoutBackendKind::CpuDx11 => {
            #[cfg(all(windows, feature = "cpu-dx11"))]
            {
                SpoutStatus::available(kind)
            }
            #[cfg(not(all(windows, feature = "cpu-dx11")))]
            {
                SpoutStatus::unavailable(kind, "CPU DX11 Spout backend is not available")
            }
        }
        SpoutBackendKind::GpuDx12Experimental => {
            #[cfg(all(windows, feature = "gpu-dx12-experimental"))]
            {
                SpoutStatus::available(kind)
            }
            #[cfg(not(all(windows, feature = "gpu-dx12-experimental")))]
            {
                SpoutStatus::unavailable(kind, "experimental DX12 Spout backend is not available")
            }
        }
    }
}
