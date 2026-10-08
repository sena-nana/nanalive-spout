//! Spout sender for frames another device exported as shared textures plus a
//! shared fence (for example NanaUI's DX12 `NativeExportPool`).
//!
//! The sender owns a D3D11 device on the exporting adapter (found by LUID),
//! opens every shared texture once per pool generation with
//! `ID3D11Device1::OpenSharedResource1` and the fence with
//! `ID3D11Device5::OpenSharedFence`, and for each frame:
//!
//! 1. `ID3D11DeviceContext4::Wait(fence, ready)` — a GPU wait, the CPU does
//!    not block;
//! 2. acquires Spout's shared-texture access, `CopyResource`s the frame's slot
//!    into Spout's shared surface, flushes and signals Spout's new frame (the
//!    [`GpuDx11TextureSender`] path);
//! 3. `ID3D11DeviceContext4::Signal(fence, release)` and `Flush`, whether the
//!    copy happened or not, so the producer can reuse the slot.
//!
//! No pixel is read or written on the CPU.

#[cfg(windows)]
use crate::SpoutPublishStatus;
#[cfg_attr(not(windows), allow(unused_imports))]
use crate::{
    GpuDx11PublishOptions, GpuDx11PublishReport, GpuDx11Status, GpuDx11TextureSender, Result,
    SpoutOutputError,
};
use core::ffi::c_void;

/// One exported frame: the shared handles of its pool and the fence values
/// that bracket the read. Every handle is borrowed for the duration of
/// [`GpuDx11SharedSender::send`] only.
#[derive(Debug, Clone, Copy)]
pub struct SharedTextureFrame<'a> {
    /// Changes whenever the handles change; the sender reopens on a new one.
    pub pool_generation: u64,
    /// `LUID` of the adapter the textures live on, `(HighPart << 32) | LowPart`.
    pub adapter_luid: i64,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// NT handles of every shared texture in the pool
    /// (`DXGI_FORMAT_B8G8R8A8_UNORM`, premultiplied).
    pub textures: &'a [*mut c_void],
    /// NT handle of the shared fence.
    pub fence: *mut c_void,
    /// Index into `textures` of this frame.
    pub slot: usize,
    /// Fence value at which the slot holds the frame.
    pub ready_value: u64,
    /// Fence value to signal once the slot is no longer read.
    pub release_value: u64,
}

/// Per-sender publish policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDx11SharedOptions {
    /// Maximum time to wait for the Spout shared-texture access lock.
    pub access_timeout_ms: u32,
    /// Collect coarse CPU-side timing around access, copy, and flush.
    pub collect_timing: bool,
}

impl Default for GpuDx11SharedOptions {
    fn default() -> Self {
        Self {
            access_timeout_ms: 1,
            collect_timing: false,
        }
    }
}

#[cfg(windows)]
struct OpenedPool {
    generation: u64,
    textures: Vec<windows::Win32::Graphics::Direct3D11::ID3D11Texture2D>,
    fence: windows::Win32::Graphics::Direct3D11::ID3D11Fence,
}

/// Publishes shared-texture frames from another device through a Spout DX11
/// sender on a device of the same adapter.
pub struct GpuDx11SharedSender {
    // Declared first: dropped before the device it was created on.
    sender: GpuDx11TextureSender,
    adapter_luid: i64,
    pool_opens: u64,
    released_without_copy: u64,
    #[cfg(windows)]
    opened: Option<OpenedPool>,
    #[cfg(windows)]
    context4: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext4,
    #[cfg(windows)]
    context: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    #[cfg(windows)]
    device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
}

impl GpuDx11SharedSender {
    /// Create a Spout sender named `name` on a new D3D11 device of the adapter
    /// `adapter_luid` names. Frames from another adapter are refused; make a
    /// new sender when the exporting adapter changes.
    pub fn new(name: &str, adapter_luid: i64) -> Result<Self> {
        #[cfg(not(windows))]
        {
            let _ = (name, adapter_luid);
            Err(SpoutOutputError::UnsupportedPlatform)
        }
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::{HMODULE, LUID};
            use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
            use windows::Win32::Graphics::Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11DeviceContext4,
            };
            use windows::Win32::Graphics::Dxgi::{
                CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, IDXGIAdapter, IDXGIFactory4,
            };
            use windows::core::Interface;

            let unavailable = |_| SpoutOutputError::DeviceInteropUnavailable;
            let factory: IDXGIFactory4 =
                CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).map_err(unavailable)?;
            let adapter: IDXGIAdapter = factory
                .EnumAdapterByLuid(LUID {
                    LowPart: adapter_luid as u32,
                    HighPart: (adapter_luid >> 32) as i32,
                })
                .map_err(unavailable)?;
            let (mut device, mut context) = (None, None);
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(unavailable)?;
            let device = device.ok_or(SpoutOutputError::DeviceInteropUnavailable)?;
            let context = context.ok_or(SpoutOutputError::DeviceInteropUnavailable)?;
            // Shared fences need D3D11.4 (Windows 10 1703+).
            let context4: ID3D11DeviceContext4 = context.cast().map_err(unavailable)?;
            let sender = GpuDx11TextureSender::new(name, device.as_raw(), context.as_raw())?;
            Ok(Self {
                sender,
                adapter_luid,
                pool_opens: 0,
                released_without_copy: 0,
                opened: None,
                context4,
                context,
                device,
            })
        }
    }

    /// The adapter this sender's device is on.
    pub fn adapter_luid(&self) -> i64 {
        self.adapter_luid
    }

    /// How many pools (handle sets) this sender has opened.
    pub fn pool_opens(&self) -> u64 {
        self.pool_opens
    }

    /// Frames whose slot was released without a copy (Spout access timed out
    /// or the copy failed).
    pub fn released_without_copy(&self) -> u64 {
        self.released_without_copy
    }

    /// Publish one shared frame: GPU-wait for its ready value, copy its slot
    /// into Spout's surface, GPU-signal its release value.
    ///
    /// The release is signalled on every path that got past opening the
    /// pool, including a skipped or failed copy, so the producer never stalls
    /// on this sender.
    ///
    /// # Safety
    ///
    /// Every handle in `frame` must be a live NT handle of the producer's
    /// pool for the duration of this call, and the producer must follow the
    /// ready/release protocol: `ready_value` is signalled on its queue after
    /// it wrote the slot, and it does not write the slot again before
    /// `release_value` completed.
    pub unsafe fn send(
        &mut self,
        frame: &SharedTextureFrame<'_>,
        options: GpuDx11SharedOptions,
    ) -> Result<GpuDx11PublishReport> {
        if frame.width == 0 || frame.height == 0 {
            return Err(SpoutOutputError::InvalidFrameDimensions {
                width: frame.width,
                height: frame.height,
            });
        }
        if frame.adapter_luid != self.adapter_luid
            || frame.slot >= frame.textures.len()
            || frame.release_value <= frame.ready_value
        {
            return Err(SpoutOutputError::DeviceInteropUnavailable);
        }
        #[cfg(not(windows))]
        {
            let _ = options;
            Err(SpoutOutputError::UnsupportedPlatform)
        }
        #[cfg(windows)]
        unsafe {
            self.open(frame)?;
            let opened = self.opened.as_ref().expect("pool was just opened");
            let texture = &opened.textures[frame.slot];
            let fence = opened.fence.clone();
            let waited = self.context4.Wait(&fence, frame.ready_value);
            let report = match waited {
                Ok(()) => {
                    use windows::core::Interface;
                    let mut publish = GpuDx11PublishOptions::bgra8(frame.width, frame.height);
                    publish.access_timeout_ms = options.access_timeout_ms;
                    publish.collect_timing = options.collect_timing;
                    self.sender.publish_texture(texture.as_raw(), publish)
                }
                Err(_) => Err(SpoutOutputError::PublishFailed),
            };
            // Release on every path: the producer must get its slot back.
            let signalled = self.context4.Signal(&fence, frame.release_value);
            self.context.Flush();
            if !matches!(
                report,
                Ok(GpuDx11PublishReport {
                    status: SpoutPublishStatus::Sent,
                    ..
                })
            ) {
                self.released_without_copy = self.released_without_copy.saturating_add(1);
            }
            signalled.map_err(|_| SpoutOutputError::PublishFailed)?;
            report
        }
    }

    #[cfg(windows)]
    unsafe fn open(&mut self, frame: &SharedTextureFrame<'_>) -> Result<()> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_TEXTURE2D_DESC, ID3D11Device1, ID3D11Device5, ID3D11Fence, ID3D11Texture2D,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        use windows::core::Interface;

        if self
            .opened
            .as_ref()
            .is_some_and(|opened| opened.generation == frame.pool_generation)
        {
            return Ok(());
        }
        self.opened = None;
        let unavailable = |_| SpoutOutputError::DeviceInteropUnavailable;
        unsafe {
            let device1: ID3D11Device1 = self.device.cast().map_err(unavailable)?;
            let device5: ID3D11Device5 = self.device.cast().map_err(unavailable)?;
            let mut textures = Vec::with_capacity(frame.textures.len());
            for handle in frame.textures {
                let texture: ID3D11Texture2D = device1
                    .OpenSharedResource1(HANDLE(*handle))
                    .map_err(unavailable)?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                texture.GetDesc(&mut desc);
                if desc.Width != frame.width
                    || desc.Height != frame.height
                    || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
                {
                    return Err(SpoutOutputError::SurfaceFormatUnsupported);
                }
                textures.push(texture);
            }
            let mut fence: Option<ID3D11Fence> = None;
            device5
                .OpenSharedFence(HANDLE(frame.fence), &mut fence)
                .map_err(unavailable)?;
            self.opened = Some(OpenedPool {
                generation: frame.pool_generation,
                textures,
                fence: fence.ok_or(SpoutOutputError::DeviceInteropUnavailable)?,
            });
        }
        self.pool_opens = self.pool_opens.saturating_add(1);
        Ok(())
    }

    /// Sender status and lifetime counters.
    pub fn status(&self) -> GpuDx11Status {
        self.sender.status()
    }

    /// Release the Spout sender and the opened pool. Harmless to repeat.
    pub fn release(&mut self) {
        self.sender.release();
        #[cfg(windows)]
        {
            self.opened = None;
        }
    }
}

impl core::fmt::Debug for GpuDx11SharedSender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GpuDx11SharedSender")
            .field("adapter_luid", &self.adapter_luid)
            .field("pool_opens", &self.pool_opens)
            .field("status", &self.status())
            .finish()
    }
}
