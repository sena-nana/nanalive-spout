#[cfg(windows)]
use crate::util::cstring;
use crate::{
    Result, SpoutBackendKind, SpoutDx12Timing, SpoutFormat, SpoutFrameRef, SpoutOutputError,
    SpoutPublishReport, SpoutPublishStatus, SpoutSenderBackend, SpoutStatus,
};
use core::ffi::c_void;
use std::time::Instant;

const DX12_WRAPPED_RESOURCE_CACHE_CAPACITY: usize = 4;

/// Raw `ID3D12Device` COM pointer marker used by the experimental DX12 API.
pub type ID3D12Device = c_void;

/// Raw `ID3D12CommandQueue` COM pointer marker used by the experimental DX12 API.
pub type ID3D12CommandQueue = c_void;

/// DX12 publish policy for NANALIVE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDx12PublishOptions {
    /// Spout shared texture access timeout in milliseconds. `0` means no wait.
    pub access_timeout_ms: u32,
    /// Collect coarse per-frame timing diagnostics.
    pub collect_timing: bool,
}

impl Default for GpuDx12PublishOptions {
    fn default() -> Self {
        Self {
            access_timeout_ms: 1,
            collect_timing: false,
        }
    }
}

/// Experimental GPU Spout sender using Spout's D3D11On12 DX12 bridge.
pub struct GpuDx12ExperimentalSender {
    #[cfg(windows)]
    raw: *mut nanalive_spout_sys::spout_dx12_t,
    wrapped: Dx12WrappedResourceCache,
    publish_options: GpuDx12PublishOptions,
    released: bool,
    width: Option<u32>,
    height: Option<u32>,
    format: SpoutFormat,
    last_error: Option<String>,
}

#[derive(Debug)]
struct Dx12WrappedResource {
    resource: *mut c_void,
    initial_state: u32,
    final_state: u32,
    width: Option<u32>,
    height: Option<u32>,
    format: SpoutFormat,
    wrapped11: *mut c_void,
}

impl Dx12WrappedResource {
    fn matches(
        &self,
        resource: *mut c_void,
        initial_state: u32,
        final_state: u32,
        width: Option<u32>,
        height: Option<u32>,
        format: SpoutFormat,
    ) -> bool {
        self.resource == resource
            && self.initial_state == initial_state
            && self.final_state == final_state
            && self.width == width
            && self.height == height
            && self.format == format
    }
}

#[derive(Debug, Default)]
struct Dx12WrappedResourceCache {
    entries: Vec<Dx12WrappedResource>,
}

impl Dx12WrappedResourceCache {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(DX12_WRAPPED_RESOURCE_CACHE_CAPACITY),
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn get(
        &mut self,
        resource: *mut c_void,
        initial_state: u32,
        final_state: u32,
        width: Option<u32>,
        height: Option<u32>,
        format: SpoutFormat,
    ) -> Option<*mut c_void> {
        let index = self.entries.iter().position(|entry| {
            entry.matches(resource, initial_state, final_state, width, height, format)
        })?;
        let entry = self.entries.remove(index);
        let wrapped11 = entry.wrapped11;
        self.entries.push(entry);
        Some(wrapped11)
    }

    fn insert(&mut self, entry: Dx12WrappedResource) -> Option<Dx12WrappedResource> {
        if let Some(index) = self.entries.iter().position(|existing| {
            existing.matches(
                entry.resource,
                entry.initial_state,
                entry.final_state,
                entry.width,
                entry.height,
                entry.format,
            )
        }) {
            return Some(core::mem::replace(&mut self.entries[index], entry));
        }

        self.entries.push(entry);
        if self.entries.len() > DX12_WRAPPED_RESOURCE_CACHE_CAPACITY {
            Some(self.entries.remove(0))
        } else {
            None
        }
    }

    fn clear(&mut self) -> Vec<Dx12WrappedResource> {
        core::mem::take(&mut self.entries)
    }
}

#[cfg(windows)]
struct WrappedResourceHandle {
    wrapped11: *mut c_void,
    wrap_us: u64,
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
                let raw = nanalive_spout_sys::spout_dx12_create();
                if raw.is_null() {
                    return Err(SpoutOutputError::BackendUnavailable);
                }
                let mut queue_ptr = queue.cast::<c_void>();
                if nanalive_spout_sys::spout_dx12_open_directx12(
                    raw,
                    device.cast::<c_void>(),
                    &mut queue_ptr,
                ) == 0
                    || nanalive_spout_sys::spout_dx12_get_d3d12_device(raw).is_null()
                {
                    nanalive_spout_sys::spout_dx12_destroy(raw);
                    return Err(SpoutOutputError::DeviceInteropUnavailable);
                }
                if nanalive_spout_sys::spout_dx12_set_sender_name(raw, cname.as_ptr()) == 0 {
                    nanalive_spout_sys::spout_dx12_destroy(raw);
                    return Err(SpoutOutputError::BackendUnavailable);
                }
                nanalive_spout_sys::spout_dx12_set_sender_format(
                    raw,
                    SpoutFormat::default().dxgi_format(),
                );
                Ok(Self {
                    raw,
                    wrapped: Dx12WrappedResourceCache::new(),
                    publish_options: GpuDx12PublishOptions::default(),
                    released: false,
                    width: None,
                    height: None,
                    format: SpoutFormat::default(),
                    last_error: None,
                })
            }
        }
    }

    /// Update DX12 publish policy.
    pub fn set_publish_options(&mut self, options: GpuDx12PublishOptions) {
        self.publish_options = options;
    }

    fn set_error(&mut self, error: SpoutOutputError) -> SpoutOutputError {
        self.last_error = Some(error.to_string());
        error
    }

    fn report(
        &mut self,
        status: SpoutPublishStatus,
        frame: Option<i64>,
        waited_us: Option<u64>,
        timing: Option<SpoutDx12Timing>,
    ) -> SpoutPublishReport {
        match status {
            SpoutPublishStatus::Sent | SpoutPublishStatus::SkippedAccessTimeout => {
                self.last_error = None;
            }
            SpoutPublishStatus::BackendUnavailable => {
                self.last_error = Some(SpoutOutputError::BackendUnavailable.to_string());
            }
            SpoutPublishStatus::InvalidFrame | SpoutPublishStatus::Failed => {
                self.last_error = Some(SpoutOutputError::PublishFailed.to_string());
            }
        }
        SpoutPublishReport {
            status,
            frame,
            waited_us,
            timing,
        }
    }

    #[cfg(windows)]
    unsafe fn release_wrapped_entries(&mut self) {
        for wrapped in self.wrapped.clear() {
            unsafe { nanalive_spout_sys::spout_dx12_release_wrapped_resource(wrapped.wrapped11) };
        }
    }

    #[cfg(windows)]
    unsafe fn ensure_wrapped_resource(
        &mut self,
        resource: *mut c_void,
        initial_state: u32,
        final_state: u32,
        collect_timing: bool,
    ) -> Result<WrappedResourceHandle> {
        if let Some(wrapped11) = self.wrapped.get(
            resource,
            initial_state,
            final_state,
            self.width,
            self.height,
            self.format,
        ) {
            return Ok(WrappedResourceHandle {
                wrapped11,
                wrap_us: 0,
            });
        }

        let wrap_start = collect_timing.then(Instant::now);
        let mut wrapped11 = core::ptr::null_mut();
        let wrapped_ok = unsafe {
            nanalive_spout_sys::spout_dx12_wrap_resource_ex(
                self.raw,
                resource,
                initial_state,
                final_state,
                &mut wrapped11,
            )
        };
        let wrap_us = wrap_start.map(elapsed_us).unwrap_or(0);
        if wrapped_ok == 0 || wrapped11.is_null() {
            return Err(self.set_error(SpoutOutputError::DeviceInteropUnavailable));
        }

        let evicted = self.wrapped.insert(Dx12WrappedResource {
            resource,
            initial_state,
            final_state,
            width: self.width,
            height: self.height,
            format: self.format,
            wrapped11,
        });
        if let Some(evicted) = evicted {
            unsafe { nanalive_spout_sys::spout_dx12_release_wrapped_resource(evicted.wrapped11) };
        }

        Ok(WrappedResourceHandle { wrapped11, wrap_us })
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
                !self.released && nanalive_spout_sys::spout_dx12_is_initialized(self.raw) != 0;
            SpoutStatus {
                available: true,
                enabled: !self.released,
                active,
                backend: Some(SpoutBackendKind::GpuDx12Experimental),
                width: if active {
                    Some(nanalive_spout_sys::spout_dx12_get_width(self.raw))
                } else {
                    self.width
                },
                height: if active {
                    Some(nanalive_spout_sys::spout_dx12_get_height(self.raw))
                } else {
                    self.height
                },
                fps: if active {
                    Some(nanalive_spout_sys::spout_dx12_get_fps(self.raw))
                } else {
                    None
                },
                frame: if active {
                    Some(nanalive_spout_sys::spout_dx12_get_frame(self.raw) as i64)
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
        #[cfg(windows)]
        if self.width != Some(width) || self.height != Some(height) || self.format != format {
            unsafe { self.release_wrapped_entries() };
        }
        self.width = Some(width);
        self.height = Some(height);
        self.format = format;

        #[cfg(windows)]
        if !self.released {
            unsafe {
                nanalive_spout_sys::spout_dx12_set_sender_format(self.raw, format.dxgi_format())
            };
        }

        Ok(())
    }

    fn publish(&mut self, frame: SpoutFrameRef<'_>) -> Result<()> {
        let report = self.publish_report(frame)?;
        match report.status {
            SpoutPublishStatus::Sent => Ok(()),
            SpoutPublishStatus::BackendUnavailable => {
                Err(self.set_error(SpoutOutputError::BackendUnavailable))
            }
            SpoutPublishStatus::InvalidFrame
            | SpoutPublishStatus::SkippedAccessTimeout
            | SpoutPublishStatus::Failed => Err(self.set_error(SpoutOutputError::PublishFailed)),
        }
    }

    fn publish_report(&mut self, frame: SpoutFrameRef<'_>) -> Result<SpoutPublishReport> {
        let total_start = self.publish_options.collect_timing.then(Instant::now);
        if self.released {
            return Ok(self.report(
                SpoutPublishStatus::BackendUnavailable,
                None,
                None,
                timing_from_parts(total_start, 0, 0, 0, 0),
            ));
        }
        let SpoutFrameRef::Dx12Resource {
            resource,
            initial_state,
            final_state,
        } = frame
        else {
            return Ok(self.report(
                SpoutPublishStatus::InvalidFrame,
                None,
                None,
                timing_from_parts(total_start, 0, 0, 0, 0),
            ));
        };
        if resource.is_null() || self.width.is_none() || self.height.is_none() {
            return Ok(self.report(
                SpoutPublishStatus::InvalidFrame,
                None,
                None,
                timing_from_parts(total_start, 0, 0, 0, 0),
            ));
        }

        #[cfg(not(windows))]
        {
            let _ = (resource, initial_state, final_state);
            Ok(self.report(
                SpoutPublishStatus::BackendUnavailable,
                None,
                None,
                timing_from_parts(total_start, 0, 0, 0, 0),
            ))
        }
        #[cfg(windows)]
        unsafe {
            let wrapped = self.ensure_wrapped_resource(
                resource,
                initial_state,
                final_state,
                self.publish_options.collect_timing,
            )?;
            let mut native = nanalive_spout_sys::spout_dx12_send_result_t::default();
            let native_ok = nanalive_spout_sys::spout_dx12_send_wrapped_resource_fast(
                self.raw,
                wrapped.wrapped11,
                self.width.expect("width checked above"),
                self.height.expect("height checked above"),
                self.format.dxgi_format(),
                self.publish_options.access_timeout_ms,
                u32::from(self.publish_options.collect_timing),
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
            let frame = (native.frame >= 0).then_some(native.frame as i64);
            let waited_us = self
                .publish_options
                .collect_timing
                .then_some(native.waited_us);
            let timing = timing_from_parts(
                total_start,
                wrapped.wrap_us,
                native.access_wait_us,
                native.submit_us,
                native.flush_us,
            );
            Ok(self.report(status, frame, waited_us, timing))
        }
    }

    fn release(&mut self) {
        if self.released {
            return;
        }
        #[cfg(windows)]
        unsafe {
            if !self.raw.is_null() {
                self.release_wrapped_entries();
                nanalive_spout_sys::spout_dx12_release_sender(self.raw);
            }
        }
        self.released = true;
    }
}

impl Drop for GpuDx12ExperimentalSender {
    fn drop(&mut self) {
        self.release();
        #[cfg(windows)]
        unsafe {
            if !self.raw.is_null() {
                nanalive_spout_sys::spout_dx12_destroy(self.raw);
            }
        }
    }
}

impl core::fmt::Debug for GpuDx12ExperimentalSender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GpuDx12ExperimentalSender")
            .field("status", &self.status())
            .field("publish_options", &self.publish_options)
            .field("wrapped_resource_cache_len", &self.wrapped.len())
            .finish()
    }
}

fn timing_from_parts(
    total_start: Option<Instant>,
    wrap_us: u64,
    access_wait_us: u64,
    submit_us: u64,
    flush_us: u64,
) -> Option<SpoutDx12Timing> {
    total_start.map(|start| SpoutDx12Timing {
        wrap_us,
        access_wait_us,
        submit_us,
        flush_us,
        total_us: elapsed_us(start),
    })
}

fn elapsed_us(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        resource: usize,
        initial_state: u32,
        final_state: u32,
        width: u32,
        height: u32,
        wrapped11: usize,
    ) -> Dx12WrappedResource {
        Dx12WrappedResource {
            resource: resource as *mut c_void,
            initial_state,
            final_state,
            width: Some(width),
            height: Some(height),
            format: SpoutFormat::B8G8R8A8_UNORM,
            wrapped11: wrapped11 as *mut c_void,
        }
    }

    #[test]
    fn wrapped_resource_cache_key_includes_final_state() {
        let resource = 0x1000usize as *mut c_void;
        let wrapped = entry(0x1000, 4, 8, 1280, 720, 0x2000);

        assert!(wrapped.matches(
            resource,
            4,
            8,
            Some(1280),
            Some(720),
            SpoutFormat::B8G8R8A8_UNORM
        ));
        assert!(!wrapped.matches(
            resource,
            4,
            16,
            Some(1280),
            Some(720),
            SpoutFormat::B8G8R8A8_UNORM
        ));
    }

    #[test]
    fn wrapped_resource_cache_hits_ring_buffer_resources() {
        let mut cache = Dx12WrappedResourceCache::new();
        cache.insert(entry(0x1000, 4, 8, 1280, 720, 0x2000));
        cache.insert(entry(0x1100, 4, 8, 1280, 720, 0x2100));
        cache.insert(entry(0x1200, 4, 8, 1280, 720, 0x2200));

        assert_eq!(
            cache.get(
                0x1100usize as *mut c_void,
                4,
                8,
                Some(1280),
                Some(720),
                SpoutFormat::B8G8R8A8_UNORM
            ),
            Some(0x2100usize as *mut c_void)
        );
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn wrapped_resource_cache_clear_releases_resize_entries_to_caller() {
        let mut cache = Dx12WrappedResourceCache::new();
        cache.insert(entry(0x1000, 4, 8, 1280, 720, 0x2000));
        cache.insert(entry(0x1100, 4, 8, 1280, 720, 0x2100));

        let cleared = cache.clear();

        assert_eq!(cleared.len(), 2);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn unsupported_frame_type_reports_invalid_frame() {
        let mut sender = GpuDx12ExperimentalSender {
            #[cfg(windows)]
            raw: core::ptr::null_mut(),
            wrapped: Dx12WrappedResourceCache::new(),
            publish_options: GpuDx12PublishOptions::default(),
            released: false,
            width: Some(1),
            height: Some(1),
            format: SpoutFormat::default(),
            last_error: None,
        };

        let report = sender
            .publish_report(SpoutFrameRef::CpuPixels {
                pixels: &[0, 0, 0, 0],
                width: 1,
                height: 1,
                pitch_bytes: None,
            })
            .expect("invalid frame should be reported, not thrown");

        assert_eq!(report.status, SpoutPublishStatus::InvalidFrame);
    }
}
