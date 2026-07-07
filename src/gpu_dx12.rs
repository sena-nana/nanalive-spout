#[cfg(windows)]
use crate::util::cstring;
use crate::{
    Result, SpoutBackendKind, SpoutFormat, SpoutFrameRef, SpoutOutputError, SpoutSenderBackend,
    SpoutStatus,
};
use core::ffi::c_void;

/// Raw `ID3D12Device` COM pointer marker used by the experimental DX12 API.
pub type ID3D12Device = c_void;

/// Raw `ID3D12CommandQueue` COM pointer marker used by the experimental DX12 API.
pub type ID3D12CommandQueue = c_void;

/// Experimental GPU Spout sender using Spout's D3D11On12 DX12 bridge.
pub struct GpuDx12ExperimentalSender {
    #[cfg(windows)]
    raw: *mut nanavts_spout_sys::spout_dx12_t,
    released: bool,
    width: Option<u32>,
    height: Option<u32>,
    format: SpoutFormat,
    last_error: Option<String>,
}

impl GpuDx12ExperimentalSender {
    /// Create an experimental DX12 sender from an existing D3D12 device and command queue.
    ///
    /// # Safety
    ///
    /// `device` must be a valid `ID3D12Device*`, and `queue` must be a valid
    /// `ID3D12CommandQueue*` for that device. Both must outlive this sender.
    pub unsafe fn with_d3d12_device_and_queue(
        name: &str,
        device: *mut ID3D12Device,
        queue: *mut ID3D12CommandQueue,
    ) -> Result<Self> {
        if device.is_null() || queue.is_null() {
            return Err(SpoutOutputError::DeviceInteropUnavailable);
        }

        #[cfg(not(windows))]
        {
            let _ = name;
            Err(SpoutOutputError::UnsupportedPlatform)
        }
        #[cfg(windows)]
        {
            let cname = cstring(name)?;
            unsafe {
                let raw = nanavts_spout_sys::spout_dx12_create();
                if raw.is_null() {
                    return Err(SpoutOutputError::BackendUnavailable);
                }
                let mut queue_ptr = queue.cast::<c_void>();
                if nanavts_spout_sys::spout_dx12_open_directx12(
                    raw,
                    device.cast::<c_void>(),
                    &mut queue_ptr,
                ) == 0
                    || nanavts_spout_sys::spout_dx12_get_d3d12_device(raw).is_null()
                {
                    nanavts_spout_sys::spout_dx12_destroy(raw);
                    return Err(SpoutOutputError::DeviceInteropUnavailable);
                }
                if nanavts_spout_sys::spout_dx12_set_sender_name(raw, cname.as_ptr()) == 0 {
                    nanavts_spout_sys::spout_dx12_destroy(raw);
                    return Err(SpoutOutputError::BackendUnavailable);
                }
                nanavts_spout_sys::spout_dx12_set_sender_format(
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

impl SpoutSenderBackend for GpuDx12ExperimentalSender {
    fn status(&self) -> SpoutStatus {
        #[cfg(not(windows))]
        {
            SpoutStatus::unavailable(
                SpoutBackendKind::GpuDx12Experimental,
                self.last_error
                    .clone()
                    .unwrap_or_else(|| SpoutOutputError::UnsupportedPlatform.to_string()),
            )
        }
        #[cfg(windows)]
        unsafe {
            let active =
                !self.released && nanavts_spout_sys::spout_dx12_is_initialized(self.raw) != 0;
            SpoutStatus {
                available: true,
                enabled: !self.released,
                active,
                backend: Some(SpoutBackendKind::GpuDx12Experimental),
                width: if active {
                    Some(nanavts_spout_sys::spout_dx12_get_width(self.raw))
                } else {
                    self.width
                },
                height: if active {
                    Some(nanavts_spout_sys::spout_dx12_get_height(self.raw))
                } else {
                    self.height
                },
                fps: if active {
                    Some(nanavts_spout_sys::spout_dx12_get_fps(self.raw))
                } else {
                    None
                },
                frame: if active {
                    Some(nanavts_spout_sys::spout_dx12_get_frame(self.raw) as i64)
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
                nanavts_spout_sys::spout_dx12_set_sender_format(self.raw, format.dxgi_format())
            };
        }

        Ok(())
    }

    fn publish(&mut self, frame: SpoutFrameRef<'_>) -> Result<()> {
        if self.released {
            return Err(self.set_error(SpoutOutputError::BackendUnavailable));
        }
        let SpoutFrameRef::Dx12Resource {
            resource,
            initial_state,
        } = frame
        else {
            return Err(self.set_error(SpoutOutputError::PublishFailed));
        };
        if resource.is_null() {
            return Err(self.set_error(SpoutOutputError::DeviceInteropUnavailable));
        }

        #[cfg(not(windows))]
        {
            let _ = (resource, initial_state);
            Err(self.set_error(SpoutOutputError::UnsupportedPlatform))
        }
        #[cfg(windows)]
        unsafe {
            let mut wrapped = core::ptr::null_mut();
            let wrapped_ok = nanavts_spout_sys::spout_dx12_wrap_resource(
                self.raw,
                resource,
                initial_state,
                &mut wrapped,
            );
            if wrapped_ok == 0 || wrapped.is_null() {
                return Err(self.set_error(SpoutOutputError::DeviceInteropUnavailable));
            }
            let send_ok = nanavts_spout_sys::spout_dx12_send_wrapped_resource(self.raw, wrapped);
            nanavts_spout_sys::spout_dx12_release_wrapped_resource(wrapped);
            if send_ok == 0 {
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
            nanavts_spout_sys::spout_dx12_release_sender(self.raw);
        }
        self.released = true;
    }
}

impl Drop for GpuDx12ExperimentalSender {
    fn drop(&mut self) {
        self.release();
        #[cfg(windows)]
        unsafe {
            nanavts_spout_sys::spout_dx12_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for GpuDx12ExperimentalSender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GpuDx12ExperimentalSender")
            .field("status", &self.status())
            .finish()
    }
}
