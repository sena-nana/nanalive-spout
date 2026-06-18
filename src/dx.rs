//! DirectX 11 backend, wrapping Spout's `spoutDX` class.
//!
//! [`Sender`] and [`Receiver`] each own their own `spoutDX` instance. The class
//! manages its own Direct3D 11 device, so you can send and receive plain pixel
//! buffers without setting up any graphics context yourself. For zero-copy GPU
//! sharing, the texture methods accept and return raw `ID3D11Texture2D` pointers
//! (as `*mut c_void`) and are `unsafe`.
//!
//! ```no_run
//! # fn main() -> spout2::Result<()> {
//! let mut sender = spout2::dx::Sender::new("My Rust Sender")?;
//! let (w, h) = (640u32, 480u32);
//! let pixels = vec![0u8; (w * h * 4) as usize]; // RGBA, 8 bits per channel
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
///
/// These are the raw `DXGI_FORMAT` enum values. `send_image` always uploads
/// 4-byte BGRA/RGBA pixels regardless; the format controls the shared texture.
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

/// A Spout DirectX 11 **sender**: publishes frames for other applications.
pub struct Sender {
    raw: *mut sys::spout_dx_t,
}

impl Sender {
    /// Create a sender with the given name, creating and owning a new D3D11 device.
    pub fn new(name: &str) -> Result<Self> {
        Self::with_name_bytes(name.as_bytes())
    }

    /// Create a sender with an arbitrary non-NUL ANSI byte name.
    ///
    /// This is useful when interoperating with non-UTF-8 sender names produced
    /// by other Spout applications.
    pub fn with_name_bytes(name: &[u8]) -> Result<Self> {
        let cname = cstring_bytes(name)?;
        // SAFETY: handle is checked for null; device is opened then verified.
        unsafe {
            let raw = sys::spout_dx_create();
            if raw.is_null() {
                return Err(SpoutError::NullHandle("spoutDX"));
            }
            sys::spout_dx_open_directx11(raw, ptr::null_mut());
            if sys::spout_dx_get_device(raw).is_null() {
                sys::spout_dx_destroy(raw);
                return Err(SpoutError::InitFailed("D3D11 device"));
            }
            sys::spout_dx_set_sender_name(raw, cname.as_ptr());
            Ok(Sender { raw })
        }
    }

    /// Create a sender that shares the caller's existing Direct3D 11 device.
    ///
    /// # Safety
    /// `device` must be a valid `ID3D11Device*` that outlives this `Sender`.
    pub unsafe fn with_device(name: &str, device: *mut c_void) -> Result<Self> {
        unsafe { Self::with_device_name_bytes(name.as_bytes(), device) }
    }

    /// Create a sender with an arbitrary non-NUL ANSI byte name and caller-owned device.
    ///
    /// # Safety
    /// `device` must be a valid `ID3D11Device*` that outlives this `Sender`.
    pub unsafe fn with_device_name_bytes(name: &[u8], device: *mut c_void) -> Result<Self> {
        let cname = cstring_bytes(name)?;
        unsafe {
            let raw = sys::spout_dx_create();
            if raw.is_null() {
                return Err(SpoutError::NullHandle("spoutDX"));
            }
            sys::spout_dx_open_directx11(raw, device);
            if sys::spout_dx_get_device(raw).is_null() {
                sys::spout_dx_destroy(raw);
                return Err(SpoutError::InitFailed("D3D11 device"));
            }
            sys::spout_dx_set_sender_name(raw, cname.as_ptr());
            Ok(Sender { raw })
        }
    }

    /// Set the shared-texture format (a `DXGI_FORMAT`; see [`format`](mod@format)).
    pub fn set_format(&mut self, dxgi_format: u32) {
        unsafe { sys::spout_dx_set_sender_format(self.raw, dxgi_format) }
    }

    /// Send a tightly-packed 4-byte-per-pixel (BGRA/RGBA) image.
    ///
    /// Returns [`SpoutError::BufferSize`] if `pixels` is smaller than
    /// `width * height * 4`.
    pub fn send_image(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<()> {
        let expected = image_byte_len(width, height, 4)?;
        if pixels.len() < expected {
            return Err(SpoutError::BufferSize {
                expected,
                got: pixels.len(),
            });
        }
        let ok = unsafe { sys::spout_dx_send_image(self.raw, pixels.as_ptr(), width, height, 0) };
        bool_result(ok, "SendImage")
    }

    /// Send an image with an explicit row pitch in bytes (`0` means tightly packed).
    ///
    /// Returns [`SpoutError::BufferSize`] if `pixels` is smaller than
    /// `row_pitch * height`, where `row_pitch` defaults to `width * 4`.
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
            unsafe { sys::spout_dx_send_image(self.raw, pixels.as_ptr(), width, height, pitch) };
        bool_result(ok, "SendImage")
    }

    /// Send a shared Direct3D 11 texture (zero-copy).
    ///
    /// # Safety
    /// `texture` must be a valid `ID3D11Texture2D*` created on this sender's
    /// device (see [`device_ptr`](Self::device_ptr)).
    pub unsafe fn send_texture(&mut self, texture: *mut c_void) -> Result<()> {
        let ok = unsafe { sys::spout_dx_send_texture(self.raw, texture) };
        bool_result(ok, "SendTexture")
    }

    /// Whether the sender has been initialized (a frame has been sent).
    pub fn is_initialized(&self) -> bool {
        unsafe { sys::spout_dx_is_initialized(self.raw) != 0 }
    }

    /// The sender's name.
    pub fn name(&self) -> String {
        String::from_utf8_lossy(&self.name_bytes()).into_owned()
    }

    /// The sender's raw ANSI name bytes.
    pub fn name_bytes(&self) -> Vec<u8> {
        read_cstr_bytes(|b, l| unsafe { sys::spout_dx_get_name(self.raw, b, l) })
    }

    /// Current sender width in pixels.
    pub fn width(&self) -> u32 {
        unsafe { sys::spout_dx_get_width(self.raw) }
    }

    /// Current sender height in pixels.
    pub fn height(&self) -> u32 {
        unsafe { sys::spout_dx_get_height(self.raw) }
    }

    /// Measured sender frame rate.
    pub fn fps(&self) -> f64 {
        unsafe { sys::spout_dx_get_fps(self.raw) }
    }

    /// Current frame number.
    pub fn frame(&self) -> i64 {
        unsafe { sys::spout_dx_get_frame(self.raw) as i64 }
    }

    /// Limit the calling loop to at most `fps` frames per second.
    ///
    /// Call once per frame (e.g. after [`send_image`](Self::send_image)); Spout
    /// sleeps as needed to hold the target rate. A value `<= 0` disables pacing.
    pub fn hold_fps(&mut self, fps: i32) {
        unsafe { sys::spout_dx_hold_fps(self.raw, fps) }
    }

    /// The underlying `ID3D11Device*` (for creating textures to send).
    pub fn device_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx_get_device(self.raw) }
    }

    /// The underlying `ID3D11DeviceContext*`.
    pub fn context_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx_get_context(self.raw) }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        unsafe {
            sys::spout_dx_release_sender(self.raw);
            sys::spout_dx_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for Sender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("dx::Sender")
            .field("name", &self.name())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("initialized", &self.is_initialized())
            .finish()
    }
}

/// A Spout DirectX 11 **receiver**: connects to a sender and reads its frames.
pub struct Receiver {
    raw: *mut sys::spout_dx_t,
}

impl Receiver {
    /// Create a receiver.
    ///
    /// Pass `Some(name)` to connect to a specific sender, or `None` to connect to
    /// the active sender. No graphics device is created until the first
    /// `receive`; enumeration ([`sender_list`](Self::sender_list)) works without one.
    pub fn new(sender_name: Option<&str>) -> Result<Self> {
        Self::with_name_bytes(sender_name.map(str::as_bytes))
    }

    /// Create a receiver from arbitrary non-NUL ANSI sender-name bytes.
    pub fn with_name_bytes(sender_name: Option<&[u8]>) -> Result<Self> {
        let cname = match sender_name {
            Some(n) => Some(cstring_bytes(n)?),
            None => None,
        };
        let raw = unsafe { sys::spout_dx_create() };
        if raw.is_null() {
            return Err(SpoutError::NullHandle("spoutDX"));
        }
        if let Some(c) = &cname {
            unsafe { sys::spout_dx_set_receiver_name(raw, c.as_ptr()) };
        }
        Ok(Receiver { raw })
    }

    /// Connect to / poll the sender, receiving into Spout's internal shared
    /// texture. Returns `true` while connected to a sender.
    ///
    /// After this, [`is_updated`](Self::is_updated), the `sender_*` getters and
    /// [`sender_texture_ptr`](Self::sender_texture_ptr) reflect the connection.
    pub fn receive(&mut self) -> bool {
        unsafe { sys::spout_dx_receive_texture(self.raw) != 0 }
    }

    /// Receive the current frame as pixels into `pixels`.
    ///
    /// `pixels` must hold at least `width * height * channels` bytes, where
    /// `channels` is 3 when `rgb` is `true`, otherwise 4. Returns `Ok(true)`
    /// while connected. The typical loop: call this, then if
    /// [`is_updated`](Self::is_updated) returns `true`, resize the buffer to
    /// [`sender_size`](Self::sender_size) and call again.
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
        let ok = unsafe {
            sys::spout_dx_receive_image(
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

    /// Receive into a caller-managed `ID3D11Texture2D`.
    ///
    /// # Safety
    /// `pp_texture` must point to a valid `ID3D11Texture2D*` slot managed per
    /// Spout's `ReceiveTexture(ID3D11Texture2D**)` contract.
    pub unsafe fn receive_into_texture(&mut self, pp_texture: *mut *mut c_void) -> bool {
        unsafe { sys::spout_dx_receive_texture_into(self.raw, pp_texture) != 0 }
    }

    /// Open Spout's sender-selection dialog (requires a desktop session).
    pub fn select_sender(&mut self) -> Result<()> {
        let ok = unsafe { sys::spout_dx_select_sender(self.raw, ptr::null_mut()) };
        bool_result(ok, "SelectSender")
    }

    /// Whether the connected sender changed size/format since the last receive.
    pub fn is_updated(&self) -> bool {
        unsafe { sys::spout_dx_is_updated(self.raw) != 0 }
    }

    /// Whether a sender is currently connected.
    pub fn is_connected(&self) -> bool {
        unsafe { sys::spout_dx_is_connected(self.raw) != 0 }
    }

    /// Whether the last received frame was new (the sender produced a new frame).
    pub fn is_frame_new(&self) -> bool {
        unsafe { sys::spout_dx_is_frame_new(self.raw) != 0 }
    }

    /// The connected sender's name.
    pub fn sender_name(&self) -> String {
        String::from_utf8_lossy(&self.sender_name_bytes()).into_owned()
    }

    /// The connected sender's raw ANSI name bytes.
    pub fn sender_name_bytes(&self) -> Vec<u8> {
        read_cstr_bytes(|b, l| unsafe { sys::spout_dx_get_sender_name(self.raw, b, l) })
    }

    /// The connected sender's width in pixels.
    pub fn sender_width(&self) -> u32 {
        unsafe { sys::spout_dx_get_sender_width(self.raw) }
    }

    /// The connected sender's height in pixels.
    pub fn sender_height(&self) -> u32 {
        unsafe { sys::spout_dx_get_sender_height(self.raw) }
    }

    /// The connected sender's `(width, height)` in pixels.
    pub fn sender_size(&self) -> (u32, u32) {
        (self.sender_width(), self.sender_height())
    }

    /// The connected sender's measured frame rate.
    pub fn sender_fps(&self) -> f64 {
        unsafe { sys::spout_dx_get_sender_fps(self.raw) }
    }

    /// The connected sender's current frame number.
    pub fn sender_frame(&self) -> i64 {
        unsafe { sys::spout_dx_get_sender_frame(self.raw) as i64 }
    }

    /// The connected sender's shared-texture `DXGI_FORMAT`.
    pub fn sender_format(&self) -> u32 {
        unsafe { sys::spout_dx_get_sender_format(self.raw) }
    }

    /// The received shared `ID3D11Texture2D*`.
    ///
    /// # Safety
    /// The pointer is owned by the receiver and only valid until the next
    /// `receive` or until this `Receiver` is dropped. Do not store it.
    pub unsafe fn sender_texture_ptr(&self) -> *mut c_void {
        unsafe { sys::spout_dx_get_sender_texture(self.raw) }
    }

    /// The received sender's DirectX share `HANDLE`.
    ///
    /// # Safety
    /// Valid only while connected to the current sender.
    pub unsafe fn sender_handle(&self) -> *mut c_void {
        unsafe { sys::spout_dx_get_sender_handle(self.raw) }
    }

    /// The number of senders currently running on the system.
    pub fn sender_count(&self) -> usize {
        let n = unsafe { sys::spout_dx_get_sender_count(self.raw) };
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
        let n = unsafe { sys::spout_dx_get_sender_count(self.raw) };
        let mut out = Vec::with_capacity(n.max(0) as usize);
        for i in 0..n {
            let name = read_cstr_bytes(|b, l| unsafe {
                sys::spout_dx_get_sender_name_at(self.raw, i, b, l)
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
        let s = read_cstr_bytes(|b, l| unsafe { sys::spout_dx_get_active_sender(self.raw, b, l) });
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
            sys::spout_dx_get_sender_info(
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
            sys::spout_dx_release_receiver(self.raw);
            sys::spout_dx_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for Receiver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("dx::Receiver")
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
