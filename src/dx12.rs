//! DirectX 12 backend, wrapping Spout's `spoutDX12` class.
//!
//! Spout shares textures between processes using **D3D11 shared handles**. The
//! DX12 backend bridges your `ID3D12Resource` textures through Microsoft's
//! **D3D11On12** layer — it is not native D3D12 shared-resource sharing.
//! DX12 senders and receivers interoperate with [`crate::dx`] and [`crate::gl`]
//! peers in the same sender namespace.
//!
//! [`Sender`] and [`Receiver`] each own a `spoutDX12` instance. The class can
//! create and own a D3D12 device, so CPU pixel send/receive works without
//! setting up a graphics context. For zero-copy GPU sharing:
//!
//! **Sender:** call [`Sender::wrap_resource`] once per D3D12 texture, then
//! [`Sender::send_wrapped_resource`] with the returned [`WrappedResource`] each
//! frame after rendering.
//!
//! **Receiver:** construct with [`Receiver::with_device`] so the receiver shares
//! your D3D12 device (the GPU receive path can only wrap textures created on the
//! same device). Then, when [`Receiver::is_updated`] is true, release any old
//! receiving texture and call [`Receiver::create_texture`], and call
//! [`Receiver::receive_resource`] each frame to copy the sender's frame. The CPU
//! pixel path ([`Receiver::receive_image`]) needs no device and works from
//! [`Receiver::new`].
//!
//! Note: with the vendored Spout 2.007.017, dropping a connected GPU receiver
//! does not release the last D3D11 resource it wrapped internally (one
//! `ID3D11Resource`); the OS reclaims it at process exit. This is an upstream
//! limitation and only matters for code that creates and drops many receivers.
//!
//! ```no_run
//! # fn main() -> spout2::Result<()> {
//! let mut sender = spout2::dx12::Sender::new("My Rust DX12 Sender")?;
//! let (w, h) = (640u32, 480u32);
//! let pixels = vec![0u8; (w * h * 4) as usize];
//! sender.send_image(&pixels, w, h)?;
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, SpoutError};
use crate::util::{cstring_bytes, image_byte_len, image_byte_len_with_row_pitch, read_cstr_bytes};
use core::ffi::c_void;
use core::ptr;
use spout2_sys as sys;

/// Common DXGI texture formats, for [`Sender::set_format`].
pub mod format {
    /// `DXGI_FORMAT_R8G8B8A8_UNORM`
    pub const R8G8B8A8_UNORM: u32 = 28;
    /// `DXGI_FORMAT_R8G8B8A8_UNORM_SRGB`
    pub const R8G8B8A8_UNORM_SRGB: u32 = 29;
    /// `DXGI_FORMAT_B8G8R8A8_UNORM` (Spout's default)
    pub const B8G8R8A8_UNORM: u32 = 87;
    /// `DXGI_FORMAT_R10G10B10A2_UNORM`
    pub const R10G10B10A2_UNORM: u32 = 24;
    /// `DXGI_FORMAT_R16G16B16A16_FLOAT`
    pub const R16G16B16A16_FLOAT: u32 = 10;
}

/// Common `D3D12_RESOURCE_STATES` values for wrap/receive helpers.
pub mod resource_state {
    /// `D3D12_RESOURCE_STATE_RENDER_TARGET`
    pub const RENDER_TARGET: u32 = 4;
    /// `D3D12_RESOURCE_STATE_PRESENT`
    pub const PRESENT: u32 = 0;
    /// `D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE`
    pub const PIXEL_SHADER_RESOURCE: u32 = 128;
    /// `D3D12_RESOURCE_STATE_COPY_DEST`
    pub const COPY_DEST: u32 = 1024;
}

/// Details about a named sender, from [`Receiver::sender_info`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SenderInfo {
    /// Sender width in pixels.
    pub width: u32,
    /// Sender height in pixels.
    pub height: u32,
    /// `DXGI_FORMAT` of the shared texture.
    pub format: u32,
    /// The DirectX shared-texture handle (an opaque `HANDLE`).
    pub share_handle: *mut c_void,
}

/// An owned D3D11On12 wrapper around a caller-created `ID3D12Resource`.
///
/// Values are returned by [`Sender::wrap_resource`] and should be reused with
/// [`Sender::send_wrapped_resource`] each frame. Dropping releases the wrapped
/// `ID3D11Resource*`.
pub struct WrappedResource {
    raw: *mut c_void,
}

impl WrappedResource {
    /// The underlying wrapped `ID3D11Resource*`.
    ///
    /// The pointer remains owned by this wrapper. Do not release it manually.
    pub fn as_ptr(&self) -> *mut c_void {
        self.raw
    }
}

impl Drop for WrappedResource {
    fn drop(&mut self) {
        unsafe {
            sys::spout_dx12_release_wrapped_resource(self.raw);
        }
        self.raw = ptr::null_mut();
    }
}

impl core::fmt::Debug for WrappedResource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("dx12::WrappedResource")
            .field("raw", &self.raw)
            .finish()
    }
}

/// A Spout DirectX 12 **sender**: publishes frames for other applications.
pub struct Sender {
    raw: *mut sys::spout_dx12_t,
}

impl Sender {
    /// Create a sender with the given name, creating and owning a new D3D12 device.
    pub fn new(name: &str) -> Result<Self> {
        Self::with_name_bytes(name.as_bytes())
    }

    /// Create a sender with an arbitrary non-NUL ANSI byte name.
    pub fn with_name_bytes(name: &[u8]) -> Result<Self> {
        let cname = cstring_bytes(name)?;
        unsafe {
            let raw = sys::spout_dx12_create();
            if raw.is_null() {
                return Err(SpoutError::NullHandle("spoutDX12"));
            }
            if sys::spout_dx12_open_directx12(raw, ptr::null_mut(), ptr::null_mut()) == 0 {
                sys::spout_dx12_destroy(raw);
                return Err(SpoutError::InitFailed("D3D12 device"));
            }
            if sys::spout_dx12_get_d3d12_device(raw).is_null() {
                sys::spout_dx12_destroy(raw);
                return Err(SpoutError::InitFailed("D3D12 device"));
            }
            sys::spout_dx12_set_sender_name(raw, cname.as_ptr());
            Ok(Sender { raw })
        }
    }

    /// Create a sender that shares the caller's existing Direct3D 12 device.
    ///
    /// # Safety
    ///
    /// `device` must be a valid `ID3D12Device*`. `command_queue` must be an
    /// `IUnknown**` pointing at the caller's `ID3D12CommandQueue*` (as Spout's
    /// `OpenDirectX12` expects). Both must outlive this `Sender`.
    pub unsafe fn with_device(
        name: &str,
        device: *mut c_void,
        command_queue: *mut *mut c_void,
    ) -> Result<Self> {
        unsafe { Self::with_device_name_bytes(name.as_bytes(), device, command_queue) }
    }

    /// Create a sender with an arbitrary non-NUL ANSI byte name and caller-owned device.
    ///
    /// # Safety
    ///
    /// Same requirements as [`with_device`](Self::with_device).
    pub unsafe fn with_device_name_bytes(
        name: &[u8],
        device: *mut c_void,
        command_queue: *mut *mut c_void,
    ) -> Result<Self> {
        let cname = cstring_bytes(name)?;
        unsafe {
            let raw = sys::spout_dx12_create();
            if raw.is_null() {
                return Err(SpoutError::NullHandle("spoutDX12"));
            }
            if sys::spout_dx12_open_directx12(raw, device, command_queue) == 0 {
                sys::spout_dx12_destroy(raw);
                return Err(SpoutError::InitFailed("D3D12 device"));
            }
            if sys::spout_dx12_get_d3d12_device(raw).is_null() {
                sys::spout_dx12_destroy(raw);
                return Err(SpoutError::InitFailed("D3D12 device"));
            }
            sys::spout_dx12_set_sender_name(raw, cname.as_ptr());
            Ok(Sender { raw })
        }
    }

    /// Set the shared-texture format (a `DXGI_FORMAT`; see [`format`](mod@format)).
    pub fn set_format(&mut self, dxgi_format: u32) {
        unsafe { sys::spout_dx12_set_sender_format(self.raw, dxgi_format) }
    }

    /// Send a tightly-packed 4-byte-per-pixel (BGRA/RGBA) image.
    pub fn send_image(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<()> {
        let expected = image_byte_len(width, height, 4)?;
        if pixels.len() < expected {
            return Err(SpoutError::BufferSize {
                expected,
                got: pixels.len(),
            });
        }
        let ok = unsafe { sys::spout_dx12_send_image(self.raw, pixels.as_ptr(), width, height, 0) };
        bool_result(ok, "SendImage")
    }

    /// Send an image with an explicit row pitch in bytes (`0` means tightly packed).
    pub fn send_image_with_pitch(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        pitch: u32,
    ) -> Result<()> {
        let row = if pitch == 0 {
            image_byte_len(width, 1, 4)?
        } else {
            pitch as usize
        };
        let expected = image_byte_len_with_row_pitch(width, height, row)?;
        if pixels.len() < expected {
            return Err(SpoutError::BufferSize {
                expected,
                got: pixels.len(),
            });
        }
        let ok =
            unsafe { sys::spout_dx12_send_image(self.raw, pixels.as_ptr(), width, height, pitch) };
        bool_result(ok, "SendImage")
    }

    /// Wrap a D3D12 texture for sending through the D3D11On12 bridge.
    ///
    /// Call once per render target. Returns an owned wrapper for the wrapped
    /// `ID3D11Resource*`.
    ///
    /// # Safety
    ///
    /// `d3d12_resource` must be a valid `ID3D12Resource*` on this sender's
    /// D3D12 device. `initial_state` is a `D3D12_RESOURCE_STATES` value (see
    /// [`resource_state`](mod@resource_state)).
    pub unsafe fn wrap_resource(
        &self,
        d3d12_resource: *mut c_void,
        initial_state: u32,
    ) -> Result<WrappedResource> {
        let mut wrapped = ptr::null_mut();
        let ok = unsafe {
            sys::spout_dx12_wrap_resource(self.raw, d3d12_resource, initial_state, &mut wrapped)
        };
        if ok != 0 && !wrapped.is_null() {
            Ok(WrappedResource { raw: wrapped })
        } else {
            Err(SpoutError::OperationFailed("WrapDX12Resource"))
        }
    }

    /// Send a wrapped D3D11On12 resource produced by [`wrap_resource`](Self::wrap_resource).
    ///
    /// # Safety
    ///
    /// `wrapped` must have been created from this sender and its underlying
    /// D3D12 resource must be in the state passed to [`wrap_resource`](Self::wrap_resource).
    pub unsafe fn send_wrapped_resource(&mut self, wrapped: &WrappedResource) -> Result<()> {
        let ok = unsafe { sys::spout_dx12_send_wrapped_resource(self.raw, wrapped.as_ptr()) };
        bool_result(ok, "SendDX11Resource")
    }

    /// Whether the sender has been initialized (a frame has been sent).
    pub fn is_initialized(&self) -> bool {
        unsafe { sys::spout_dx12_is_initialized(self.raw) != 0 }
    }

    /// The sender's name.
    pub fn name(&self) -> String {
        String::from_utf8_lossy(&self.name_bytes()).into_owned()
    }

    /// The sender's raw ANSI name bytes.
    pub fn name_bytes(&self) -> Vec<u8> {
        read_cstr_bytes(|b, l| unsafe { sys::spout_dx12_get_name(self.raw, b, l) })
    }

    /// Current sender width in pixels.
    pub fn width(&self) -> u32 {
        unsafe { sys::spout_dx12_get_width(self.raw) }
    }

    /// Current sender height in pixels.
    pub fn height(&self) -> u32 {
        unsafe { sys::spout_dx12_get_height(self.raw) }
    }

    /// Measured sender frame rate.
    pub fn fps(&self) -> f64 {
        unsafe { sys::spout_dx12_get_fps(self.raw) }
    }

    /// Current frame number.
    pub fn frame(&self) -> i64 {
        unsafe { sys::spout_dx12_get_frame(self.raw) as i64 }
    }

    /// Limit the calling loop to at most `fps` frames per second.
    pub fn hold_fps(&mut self, fps: i32) {
        unsafe { sys::spout_dx12_hold_fps(self.raw, fps) }
    }

    /// The underlying `ID3D12Device*`.
    pub fn d3d12_device_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_d3d12_device(self.raw) }
    }

    /// The D3D11 device from the 11-on-12 bridge (`ID3D11Device*`).
    pub fn device_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_device(self.raw) }
    }

    /// The D3D11 context from the 11-on-12 bridge (`ID3D11DeviceContext*`).
    pub fn context_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_context(self.raw) }
    }

    /// The underlying `ID3D11On12Device*`.
    pub fn d3d11on12_device_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_d3d11on12_device(self.raw) }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        unsafe {
            sys::spout_dx12_release_sender(self.raw);
            sys::spout_dx12_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for Sender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("dx12::Sender")
            .field("name", &self.name())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("initialized", &self.is_initialized())
            .finish()
    }
}

/// A Spout DirectX 12 **receiver**: connects to a sender and reads its frames.
pub struct Receiver {
    raw: *mut sys::spout_dx12_t,
    device_open: bool,
}

impl Receiver {
    /// Create a receiver.
    ///
    /// Pass `Some(name)` to connect to a specific sender, or `None` to connect to
    /// the active sender. A Spout-owned graphics device is opened lazily on the
    /// first receive call; enumeration ([`sender_list`](Self::sender_list)) works
    /// without one. For the GPU receive path
    /// ([`receive_resource`](Self::receive_resource)), construct with
    /// [`with_device`](Self::with_device) instead so the receiver shares your
    /// D3D12 device.
    pub fn new(sender_name: Option<&str>) -> Result<Self> {
        Self::with_name_bytes(sender_name.map(str::as_bytes))
    }

    /// Create a receiver from arbitrary non-NUL ANSI sender-name bytes.
    pub fn with_name_bytes(sender_name: Option<&[u8]>) -> Result<Self> {
        let cname = match sender_name {
            Some(n) => Some(cstring_bytes(n)?),
            None => None,
        };
        let raw = unsafe { sys::spout_dx12_create() };
        if raw.is_null() {
            return Err(SpoutError::NullHandle("spoutDX12"));
        }
        if let Some(c) = &cname {
            unsafe { sys::spout_dx12_set_receiver_name(raw, c.as_ptr()) };
        }
        Ok(Receiver {
            raw,
            device_open: false,
        })
    }

    /// Create a receiver that shares the caller's existing Direct3D 12 device.
    ///
    /// **Required for the GPU receive path.** [`receive_resource`](Self::receive_resource)
    /// wraps the D3D12 texture you pass through Spout's D3D11On12 device, and
    /// D3D11On12 can only wrap a resource that belongs to the *same* D3D12 device
    /// it was created from. So the texture you build with
    /// [`create_texture`](Self::create_texture) and this receiver must live on one
    /// device — pass it here. The default [`new`](Self::new) constructor lazily
    /// creates a separate Spout-owned device, which works for the CPU
    /// [`receive_image`](Self::receive_image) path but cannot wrap textures you
    /// created on your own device.
    ///
    /// `sender_name` selects the sender to connect to (`None` = the active sender),
    /// matching [`new`](Self::new).
    ///
    /// # Safety
    ///
    /// `device` must be a valid `ID3D12Device*`. `command_queue` must be an
    /// `IUnknown**` pointing at the caller's `ID3D12CommandQueue*` (as Spout's
    /// `OpenDirectX12` expects). Both must outlive this `Receiver`.
    pub unsafe fn with_device(
        sender_name: Option<&str>,
        device: *mut c_void,
        command_queue: *mut *mut c_void,
    ) -> Result<Self> {
        unsafe {
            Self::with_device_name_bytes(sender_name.map(str::as_bytes), device, command_queue)
        }
    }

    /// Create a device-sharing receiver from arbitrary non-NUL ANSI sender-name bytes.
    ///
    /// # Safety
    ///
    /// Same requirements as [`with_device`](Self::with_device).
    pub unsafe fn with_device_name_bytes(
        sender_name: Option<&[u8]>,
        device: *mut c_void,
        command_queue: *mut *mut c_void,
    ) -> Result<Self> {
        let cname = match sender_name {
            Some(n) => Some(cstring_bytes(n)?),
            None => None,
        };
        unsafe {
            let raw = sys::spout_dx12_create();
            if raw.is_null() {
                return Err(SpoutError::NullHandle("spoutDX12"));
            }
            if sys::spout_dx12_open_directx12(raw, device, command_queue) == 0 {
                sys::spout_dx12_destroy(raw);
                return Err(SpoutError::InitFailed("D3D12 device"));
            }
            if sys::spout_dx12_get_d3d12_device(raw).is_null() {
                sys::spout_dx12_destroy(raw);
                return Err(SpoutError::InitFailed("D3D12 device"));
            }
            if let Some(c) = &cname {
                sys::spout_dx12_set_receiver_name(raw, c.as_ptr());
            }
            Ok(Receiver {
                raw,
                device_open: true,
            })
        }
    }

    fn ensure_device(&mut self) -> Result<()> {
        if self.device_open {
            return Ok(());
        }
        let ok =
            unsafe { sys::spout_dx12_open_directx12(self.raw, ptr::null_mut(), ptr::null_mut()) };
        if ok == 0 {
            return Err(SpoutError::InitFailed("D3D12 device"));
        }
        self.device_open = true;
        Ok(())
    }

    /// Receive the current frame as pixels into `pixels`.
    pub fn receive_image(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        rgb: bool,
        invert: bool,
    ) -> Result<bool> {
        let channels = if rgb { 3 } else { 4 };
        let expected = image_byte_len(width, height, channels)?;
        if pixels.len() < expected {
            return Err(SpoutError::BufferSize {
                expected,
                got: pixels.len(),
            });
        }
        self.ensure_device()?;
        let ok = unsafe {
            sys::spout_dx12_receive_image(
                self.raw,
                pixels.as_mut_ptr(),
                width,
                height,
                rgb as i32,
                invert as i32,
            )
        };
        Ok(ok != 0)
    }

    /// Receive a sender's frame into a D3D12 texture resource.
    ///
    /// Returns `true` while connected. When [`is_updated`](Self::is_updated) is
    /// `true`, release the old texture and call [`create_texture`](Self::create_texture)
    /// before the next successful receive.
    ///
    /// # Safety
    ///
    /// `pp_d3d12_texture` must point to a valid `ID3D12Resource*` slot (i.e.
    /// `ID3D12Resource**`). The pointed-to texture must be created on this
    /// receiver's D3D12 device — use [`with_device`](Self::with_device) so that is
    /// your device — for receiving (typically `COPY_DEST` state). The slot itself
    /// may hold null before the first [`is_updated`](Self::is_updated)/`create_texture`
    /// cycle; the call connects without copying in that case.
    pub unsafe fn receive_resource(&mut self, pp_d3d12_texture: *mut *mut c_void) -> Result<bool> {
        self.ensure_device()?;
        Ok(unsafe { sys::spout_dx12_receive_resource(self.raw, pp_d3d12_texture) != 0 })
    }

    /// Create a D3D12 texture for receiving sender frames.
    ///
    /// Call after [`is_updated`](Self::is_updated) when the sender size or format
    /// changes.
    ///
    /// # Safety
    ///
    /// `device` must be a valid `ID3D12Device*`. The returned `ID3D12Resource*`
    /// is owned by the caller.
    pub unsafe fn create_texture(
        &self,
        device: *mut c_void,
        width: u32,
        height: u32,
        initial_state: u32,
        format: u32,
    ) -> Result<*mut c_void> {
        let mut tex = ptr::null_mut();
        let ok = unsafe {
            sys::spout_dx12_create_texture(
                self.raw,
                device,
                width,
                height,
                initial_state,
                format,
                &mut tex,
            )
        };
        if ok != 0 {
            Ok(tex)
        } else {
            Err(SpoutError::OperationFailed("CreateDX12texture"))
        }
    }

    /// Open Spout's sender-selection dialog (requires a desktop session).
    pub fn select_sender(&mut self) -> Result<()> {
        let ok = unsafe { sys::spout_dx12_select_sender(self.raw, ptr::null_mut()) };
        bool_result(ok, "SelectSender")
    }

    /// Whether the connected sender changed size/format since the last receive.
    pub fn is_updated(&self) -> bool {
        unsafe { sys::spout_dx12_is_updated(self.raw) != 0 }
    }

    /// Whether a sender is currently connected.
    pub fn is_connected(&self) -> bool {
        unsafe { sys::spout_dx12_is_connected(self.raw) != 0 }
    }

    /// Whether the last received frame was new (the sender produced a new frame).
    pub fn is_frame_new(&self) -> bool {
        unsafe { sys::spout_dx12_is_frame_new(self.raw) != 0 }
    }

    /// The connected sender's name.
    pub fn sender_name(&self) -> String {
        String::from_utf8_lossy(&self.sender_name_bytes()).into_owned()
    }

    /// The connected sender's raw ANSI name bytes.
    pub fn sender_name_bytes(&self) -> Vec<u8> {
        read_cstr_bytes(|b, l| unsafe { sys::spout_dx12_get_sender_name(self.raw, b, l) })
    }

    /// The connected sender's width in pixels.
    pub fn sender_width(&self) -> u32 {
        unsafe { sys::spout_dx12_get_sender_width(self.raw) }
    }

    /// The connected sender's height in pixels.
    pub fn sender_height(&self) -> u32 {
        unsafe { sys::spout_dx12_get_sender_height(self.raw) }
    }

    /// The connected sender's `(width, height)` in pixels.
    pub fn sender_size(&self) -> (u32, u32) {
        (self.sender_width(), self.sender_height())
    }

    /// The connected sender's measured frame rate.
    pub fn sender_fps(&self) -> f64 {
        unsafe { sys::spout_dx12_get_sender_fps(self.raw) }
    }

    /// The connected sender's current frame number.
    pub fn sender_frame(&self) -> i64 {
        unsafe { sys::spout_dx12_get_sender_frame(self.raw) as i64 }
    }

    /// The connected sender's shared-texture `DXGI_FORMAT`.
    pub fn sender_format(&self) -> u32 {
        unsafe { sys::spout_dx12_get_sender_format(self.raw) }
    }

    /// The received sender's DirectX share `HANDLE`.
    ///
    /// # Safety
    ///
    /// Valid only while connected to the current sender.
    pub unsafe fn sender_handle(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_sender_handle(self.raw) }
    }

    /// The underlying `ID3D12Device*` (valid after the device is opened).
    pub fn d3d12_device_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_d3d12_device(self.raw) }
    }

    /// The D3D11 device from the 11-on-12 bridge (`ID3D11Device*`).
    pub fn device_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_device(self.raw) }
    }

    /// The D3D11 context from the 11-on-12 bridge (`ID3D11DeviceContext*`).
    pub fn context_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx12_get_context(self.raw) }
    }

    /// The number of senders currently running on the system.
    pub fn sender_count(&self) -> usize {
        let n = unsafe { sys::spout_dx12_get_sender_count(self.raw) };
        n.max(0) as usize
    }

    /// The names of all senders currently running on the system.
    pub fn sender_list(&self) -> Vec<String> {
        self.sender_list_bytes()
            .into_iter()
            .map(|name| String::from_utf8_lossy(&name).into_owned())
            .collect()
    }

    /// The raw ANSI names of all senders currently running on the system.
    pub fn sender_list_bytes(&self) -> Vec<Vec<u8>> {
        let n = unsafe { sys::spout_dx12_get_sender_count(self.raw) };
        let mut out = Vec::with_capacity(n.max(0) as usize);
        for i in 0..n {
            let name = read_cstr_bytes(|b, l| unsafe {
                sys::spout_dx12_get_sender_name_at(self.raw, i, b, l)
            });
            if !name.is_empty() {
                out.push(name);
            }
        }
        out
    }

    /// The active sender's name, if any.
    pub fn active_sender(&self) -> Option<String> {
        self.active_sender_bytes()
            .map(|s| String::from_utf8_lossy(&s).into_owned())
    }

    /// The active sender's raw ANSI name bytes, if any.
    pub fn active_sender_bytes(&self) -> Option<Vec<u8>> {
        let s =
            read_cstr_bytes(|b, l| unsafe { sys::spout_dx12_get_active_sender(self.raw, b, l) });
        if s.is_empty() { None } else { Some(s) }
    }

    /// Look up details for a named sender without connecting to it.
    pub fn sender_info(&self, name: &str) -> Result<SenderInfo> {
        self.sender_info_bytes(name.as_bytes())
    }

    /// Look up details for a sender by arbitrary non-NUL ANSI name bytes.
    pub fn sender_info_bytes(&self, name: &[u8]) -> Result<SenderInfo> {
        let cname = cstring_bytes(name)?;
        let mut info = SenderInfo {
            width: 0,
            height: 0,
            format: 0,
            share_handle: ptr::null_mut(),
        };
        let ok = unsafe {
            sys::spout_dx12_get_sender_info(
                self.raw,
                cname.as_ptr(),
                &mut info.width,
                &mut info.height,
                &mut info.share_handle,
                &mut info.format,
            )
        };
        if ok != 0 {
            Ok(info)
        } else {
            Err(SpoutError::OperationFailed("GetSenderInfo"))
        }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        unsafe {
            sys::spout_dx12_release_receiver(self.raw);
            sys::spout_dx12_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for Receiver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("dx12::Receiver")
            .field("sender_name", &self.sender_name())
            .field("connected", &self.is_connected())
            .finish()
    }
}

#[inline]
fn bool_result(ok: i32, op: &'static str) -> Result<()> {
    if ok != 0 {
        Ok(())
    } else {
        Err(SpoutError::OperationFailed(op))
    }
}
