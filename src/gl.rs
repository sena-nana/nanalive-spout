//! OpenGL backend, wrapping Spout's `Spout` class.
//!
//! [`Sender`] and [`Receiver`] each own their own `Spout` instance. OpenGL calls
//! require a current GL context on the calling thread, so these types are
//! created either with a hidden context Spout manages for you
//! ([`Sender::with_hidden_context`]) or against a context you already made
//! current ([`Sender::in_current_context`], `unsafe`). Because a GL context is
//! thread-bound, these types are `!Send`/`!Sync` — use them only on the thread
//! that created them.
//!
//! To list running senders without any GL context, use the free function
//! [`sender_names`].
//!
//! ```no_run
//! # fn main() -> spout2::Result<()> {
//! let mut sender = spout2::gl::Sender::with_hidden_context("My GL Sender")?;
//! let (w, h) = (640u32, 480u32);
//! let pixels = vec![0u8; (w * h * 4) as usize]; // RGBA
//! sender.send_image(&pixels, w, h)?;
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, SpoutError};
use crate::util::{cstring_bytes, image_byte_len, read_cstr_bytes};
use core::ffi::c_void;
use core::ptr;
use spout2_sys as sys;

/// OpenGL pixel formats accepted by the image send/receive methods.
pub mod format {
    /// `GL_RGB` (3 bytes per pixel).
    pub const RGB: u32 = 0x1907;
    /// `GL_RGBA` (4 bytes per pixel).
    pub const RGBA: u32 = 0x1908;
    /// `GL_BGR` (3 bytes per pixel).
    pub const BGR: u32 = 0x80E0;
    /// `GL_BGRA` (4 bytes per pixel).
    pub const BGRA: u32 = 0x80E1;
}

/// Bytes per pixel for a supported [`format`](mod@format) (defaults to 4 for unknown values).
fn channels(gl_format: u32) -> usize {
    match gl_format {
        format::RGB | format::BGR => 3,
        _ => 4,
    }
}

/// Details about a named sender, from [`Receiver::sender_info`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SenderInfo {
    /// Sender width in pixels.
    pub width: u32,
    /// Sender height in pixels.
    pub height: u32,
    /// `DXGI_FORMAT` of the shared texture (Spout shares via DirectX internally).
    pub format: u32,
    /// The DirectX shared-texture handle (an opaque `HANDLE`).
    pub share_handle: *mut c_void,
}

/// List the names of all Spout senders currently running on the system.
///
/// This reads only shared memory and needs no GL context or GPU.
pub fn sender_names() -> Vec<String> {
    sender_names_bytes()
        .into_iter()
        .map(|name| String::from_utf8_lossy(&name).into_owned())
        .collect()
}

/// List the raw ANSI names of all Spout senders currently running on the system.
///
/// This reads only shared memory and needs no GL context or GPU.
pub fn sender_names_bytes() -> Vec<Vec<u8>> {
    // Enumeration touches only the shared-memory sender registry; the temporary
    // Spout instance is destroyed before returning.
    let h = unsafe { sys::spout_gl_create() };
    if h.is_null() {
        return Vec::new();
    }
    let n = unsafe { sys::spout_gl_get_sender_count(h) };
    let mut out = Vec::with_capacity(n.max(0) as usize);
    for i in 0..n {
        let name = read_cstr_bytes(|b, l| unsafe { sys::spout_gl_get_sender_name_at(h, i, b, l) });
        if !name.is_empty() {
            out.push(name);
        }
    }
    unsafe { sys::spout_gl_destroy(h) };
    out
}

/// A Spout OpenGL **sender**: publishes frames for other applications.
pub struct Sender {
    raw: *mut sys::spout_gl_t,
    owns_context: bool,
}

impl Sender {
    /// Create a sender, letting Spout create and own a hidden OpenGL context.
    ///
    /// Use this when your application does not already have a current GL context
    /// (e.g. a headless pixel-buffer pipeline).
    ///
    /// Only one hidden context exists per thread: if a GL context is already
    /// current (including one created by another `with_hidden_context` object on
    /// this thread), Spout reuses it rather than creating a second. Prefer a
    /// single hidden-context object per thread.
    pub fn with_hidden_context(name: &str) -> Result<Self> {
        Self::with_hidden_context_bytes(name.as_bytes())
    }

    /// Create a sender with an arbitrary non-NUL ANSI byte name and hidden context.
    pub fn with_hidden_context_bytes(name: &[u8]) -> Result<Self> {
        let cname = cstring_bytes(name)?;
        let raw = unsafe { sys::spout_gl_create() };
        if raw.is_null() {
            return Err(SpoutError::NullHandle("Spout"));
        }
        if unsafe { sys::spout_gl_create_opengl(raw, ptr::null_mut()) } == 0 {
            unsafe { sys::spout_gl_destroy(raw) };
            return Err(SpoutError::InitFailed("OpenGL context"));
        }
        unsafe { sys::spout_gl_set_sender_name(raw, cname.as_ptr()) };
        Ok(Sender {
            raw,
            owns_context: true,
        })
    }

    /// Create a sender that uses the caller's already-current OpenGL context.
    ///
    /// # Safety
    /// A valid OpenGL context must be current on the calling thread for the
    /// entire lifetime of this `Sender` and all of its method calls.
    pub unsafe fn in_current_context(name: &str) -> Result<Self> {
        unsafe { Self::in_current_context_bytes(name.as_bytes()) }
    }

    /// Create a sender with an arbitrary non-NUL ANSI byte name in the current context.
    ///
    /// # Safety
    /// A valid OpenGL context must be current on the calling thread for the
    /// entire lifetime of this `Sender` and all of its method calls.
    pub unsafe fn in_current_context_bytes(name: &[u8]) -> Result<Self> {
        let cname = cstring_bytes(name)?;
        let raw = unsafe { sys::spout_gl_create() };
        if raw.is_null() {
            return Err(SpoutError::NullHandle("Spout"));
        }
        unsafe { sys::spout_gl_set_sender_name(raw, cname.as_ptr()) };
        Ok(Sender {
            raw,
            owns_context: false,
        })
    }

    /// Set the shared-texture `DXGI_FORMAT` (Spout shares via DirectX internally).
    pub fn set_format(&mut self, dxgi_format: u32) {
        unsafe { sys::spout_gl_set_sender_format(self.raw, dxgi_format) }
    }

    /// Send a tightly-packed RGBA image (`GL_RGBA`, not inverted).
    pub fn send_image(&mut self, pixels: &[u8], width: u32, height: u32) -> Result<()> {
        self.send_image_with(pixels, width, height, format::RGBA, false)
    }

    /// Send an image in the given [`format`](mod@format), optionally vertically inverted.
    ///
    /// Returns [`SpoutError::BufferSize`] if `pixels` is smaller than
    /// `width * height * channels`.
    pub fn send_image_with(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        gl_format: u32,
        invert: bool,
    ) -> Result<()> {
        let expected = image_byte_len(width, height, channels(gl_format))?;
        if pixels.len() < expected {
            return Err(SpoutError::BufferSize {
                expected,
                got: pixels.len(),
            });
        }
        let ok = unsafe {
            sys::spout_gl_send_image(
                self.raw,
                pixels.as_ptr(),
                width,
                height,
                gl_format,
                invert as i32,
                0,
            )
        };
        bool_result(ok, "SendImage")
    }

    /// Send an OpenGL texture (zero-copy).
    ///
    /// # Safety
    /// `texture`/`target` must be valid GL names in the current context, and
    /// `host_fbo` must be the currently-bound draw FBO (0 for the default).
    pub unsafe fn send_texture(
        &mut self,
        texture: u32,
        target: u32,
        width: u32,
        height: u32,
        invert: bool,
        host_fbo: u32,
    ) -> Result<()> {
        let ok = unsafe {
            sys::spout_gl_send_texture(
                self.raw,
                texture,
                target,
                width,
                height,
                invert as i32,
                host_fbo,
            )
        };
        bool_result(ok, "SendTexture")
    }

    /// Send the contents of an OpenGL framebuffer object.
    ///
    /// # Safety
    /// `fbo` must be a valid FBO (or 0 for the default framebuffer) in the
    /// current context.
    pub unsafe fn send_fbo(
        &mut self,
        fbo: u32,
        width: u32,
        height: u32,
        invert: bool,
    ) -> Result<()> {
        let ok = unsafe { sys::spout_gl_send_fbo(self.raw, fbo, width, height, invert as i32) };
        bool_result(ok, "SendFbo")
    }

    /// Whether the sender has been initialized (a frame has been sent).
    pub fn is_initialized(&self) -> bool {
        unsafe { sys::spout_gl_is_initialized(self.raw) != 0 }
    }

    /// The sender's name.
    pub fn name(&self) -> String {
        String::from_utf8_lossy(&self.name_bytes()).into_owned()
    }

    /// The sender's raw ANSI name bytes.
    pub fn name_bytes(&self) -> Vec<u8> {
        read_cstr_bytes(|b, l| unsafe { sys::spout_gl_get_name(self.raw, b, l) })
    }

    /// Current sender width in pixels.
    pub fn width(&self) -> u32 {
        unsafe { sys::spout_gl_get_width(self.raw) }
    }

    /// Current sender height in pixels.
    pub fn height(&self) -> u32 {
        unsafe { sys::spout_gl_get_height(self.raw) }
    }

    /// Measured sender frame rate.
    pub fn fps(&self) -> f64 {
        unsafe { sys::spout_gl_get_fps(self.raw) }
    }

    /// Current frame number.
    pub fn frame(&self) -> i64 {
        unsafe { sys::spout_gl_get_frame(self.raw) as i64 }
    }

    /// Limit the calling loop to at most `fps` frames per second.
    ///
    /// Call once per frame (e.g. after [`send_image`](Self::send_image)); Spout
    /// sleeps as needed to hold the target rate. A value `<= 0` disables pacing.
    pub fn hold_fps(&mut self, fps: i32) {
        unsafe { sys::spout_gl_hold_fps(self.raw, fps) }
    }

    /// The DirectX share `HANDLE` backing this sender.
    ///
    /// # Safety
    /// Valid only while the sender is initialized.
    pub unsafe fn share_handle(&self) -> *mut c_void {
        unsafe { sys::spout_gl_get_handle(self.raw) }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        unsafe {
            sys::spout_gl_release_sender(self.raw);
            if self.owns_context {
                sys::spout_gl_close_opengl(self.raw);
            }
            sys::spout_gl_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for Sender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("gl::Sender")
            .field("name", &self.name())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("initialized", &self.is_initialized())
            .field("owns_context", &self.owns_context)
            .finish()
    }
}

/// A Spout OpenGL **receiver**: connects to a sender and reads its frames.
pub struct Receiver {
    raw: *mut sys::spout_gl_t,
    owns_context: bool,
}

impl Receiver {
    /// Create a receiver with a hidden OpenGL context managed by Spout.
    ///
    /// Pass `Some(name)` to connect to a specific sender, or `None` for the
    /// active sender.
    ///
    /// Only one hidden context exists per thread: if a GL context is already
    /// current (including one created by another `with_hidden_context` object on
    /// this thread), Spout reuses it rather than creating a second. Prefer a
    /// single hidden-context object per thread.
    pub fn with_hidden_context(sender_name: Option<&str>) -> Result<Self> {
        Self::with_hidden_context_bytes(sender_name.map(str::as_bytes))
    }

    /// Create a receiver from arbitrary non-NUL ANSI sender-name bytes.
    pub fn with_hidden_context_bytes(sender_name: Option<&[u8]>) -> Result<Self> {
        let cname = match sender_name {
            Some(n) => Some(cstring_bytes(n)?),
            None => None,
        };
        let raw = unsafe { sys::spout_gl_create() };
        if raw.is_null() {
            return Err(SpoutError::NullHandle("Spout"));
        }
        if unsafe { sys::spout_gl_create_opengl(raw, ptr::null_mut()) } == 0 {
            unsafe { sys::spout_gl_destroy(raw) };
            return Err(SpoutError::InitFailed("OpenGL context"));
        }
        if let Some(c) = &cname {
            unsafe { sys::spout_gl_set_receiver_name(raw, c.as_ptr()) };
        }
        Ok(Receiver {
            raw,
            owns_context: true,
        })
    }

    /// Create a receiver that uses the caller's already-current OpenGL context.
    ///
    /// # Safety
    /// A valid OpenGL context must be current on the calling thread for the
    /// entire lifetime of this `Receiver` and all of its method calls.
    pub unsafe fn in_current_context(sender_name: Option<&str>) -> Result<Self> {
        unsafe { Self::in_current_context_bytes(sender_name.map(str::as_bytes)) }
    }

    /// Create a receiver from arbitrary non-NUL ANSI name bytes in the current context.
    ///
    /// # Safety
    /// A valid OpenGL context must be current on the calling thread for the
    /// entire lifetime of this `Receiver` and all of its method calls.
    pub unsafe fn in_current_context_bytes(sender_name: Option<&[u8]>) -> Result<Self> {
        let cname = match sender_name {
            Some(n) => Some(cstring_bytes(n)?),
            None => None,
        };
        let raw = unsafe { sys::spout_gl_create() };
        if raw.is_null() {
            return Err(SpoutError::NullHandle("Spout"));
        }
        if let Some(c) = &cname {
            unsafe { sys::spout_gl_set_receiver_name(raw, c.as_ptr()) };
        }
        Ok(Receiver {
            raw,
            owns_context: false,
        })
    }

    /// Connect to / poll the sender without copying. Returns `true` while
    /// connected. Use this to discover the sender and its size before allocating
    /// a pixel buffer; then check [`is_updated`](Self::is_updated) and
    /// [`sender_size`](Self::sender_size).
    pub fn receive(&mut self) -> bool {
        unsafe { sys::spout_gl_receive(self.raw) != 0 }
    }

    /// Receive the current frame as pixels into `pixels`, in the given [`format`](mod@format).
    ///
    /// `pixels` must hold at least `sender_width * sender_height * channels`
    /// bytes. The usual loop: call this, and if [`is_updated`](Self::is_updated)
    /// returns `true`, resize the buffer to [`sender_size`](Self::sender_size)
    /// and call again. Returns `Ok(true)` while connected.
    pub fn receive_image(
        &mut self,
        pixels: &mut [u8],
        gl_format: u32,
        invert: bool,
    ) -> Result<bool> {
        let (w, h) = self.sender_size();
        let expected = image_byte_len(w, h, channels(gl_format))?;
        if expected > 0 && pixels.len() < expected {
            return Err(SpoutError::BufferSize {
                expected,
                got: pixels.len(),
            });
        }
        let ok = unsafe {
            sys::spout_gl_receive_image(self.raw, pixels.as_mut_ptr(), gl_format, invert as i32, 0)
        };
        Ok(ok != 0)
    }

    /// Receive the shared texture into a caller-owned OpenGL texture.
    ///
    /// # Safety
    /// `texture`/`target` must be valid GL names in the current context sized to
    /// the sender (re-check after [`is_updated`](Self::is_updated)); `host_fbo`
    /// is the currently-bound draw FBO (0 for the default).
    pub unsafe fn receive_texture(
        &mut self,
        texture: u32,
        target: u32,
        invert: bool,
        host_fbo: u32,
    ) -> bool {
        unsafe {
            sys::spout_gl_receive_texture(self.raw, texture, target, invert as i32, host_fbo) != 0
        }
    }

    /// Open Spout's sender-selection dialog (requires a desktop session).
    pub fn select_sender(&mut self) -> Result<()> {
        let ok = unsafe { sys::spout_gl_select_sender(self.raw, ptr::null_mut()) };
        bool_result(ok, "SelectSender")
    }

    /// Whether the connected sender changed size/format since the last receive.
    pub fn is_updated(&self) -> bool {
        unsafe { sys::spout_gl_is_updated(self.raw) != 0 }
    }

    /// Whether a sender is currently connected.
    pub fn is_connected(&self) -> bool {
        unsafe { sys::spout_gl_is_connected(self.raw) != 0 }
    }

    /// Whether the last received frame was new.
    pub fn is_frame_new(&self) -> bool {
        unsafe { sys::spout_gl_is_frame_new(self.raw) != 0 }
    }

    /// The connected sender's name.
    pub fn sender_name(&self) -> String {
        String::from_utf8_lossy(&self.sender_name_bytes()).into_owned()
    }

    /// The connected sender's raw ANSI name bytes.
    pub fn sender_name_bytes(&self) -> Vec<u8> {
        read_cstr_bytes(|b, l| unsafe { sys::spout_gl_get_sender_name(self.raw, b, l) })
    }

    /// The connected sender's width in pixels.
    pub fn sender_width(&self) -> u32 {
        unsafe { sys::spout_gl_get_sender_width(self.raw) }
    }

    /// The connected sender's height in pixels.
    pub fn sender_height(&self) -> u32 {
        unsafe { sys::spout_gl_get_sender_height(self.raw) }
    }

    /// The connected sender's `(width, height)` in pixels.
    pub fn sender_size(&self) -> (u32, u32) {
        (self.sender_width(), self.sender_height())
    }

    /// The connected sender's measured frame rate.
    pub fn sender_fps(&self) -> f64 {
        unsafe { sys::spout_gl_get_sender_fps(self.raw) }
    }

    /// The connected sender's current frame number.
    pub fn sender_frame(&self) -> i64 {
        unsafe { sys::spout_gl_get_sender_frame(self.raw) as i64 }
    }

    /// The connected sender's shared-texture `DXGI_FORMAT`.
    pub fn sender_format(&self) -> u32 {
        unsafe { sys::spout_gl_get_sender_format(self.raw) }
    }

    /// The connected sender's DirectX share `HANDLE`.
    ///
    /// # Safety
    /// Valid only while connected to the current sender.
    pub unsafe fn sender_handle(&self) -> *mut c_void {
        unsafe { sys::spout_gl_get_sender_handle(self.raw) }
    }

    /// The number of senders currently running on the system.
    pub fn sender_count(&self) -> usize {
        let n = unsafe { sys::spout_gl_get_sender_count(self.raw) };
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
        let n = unsafe { sys::spout_gl_get_sender_count(self.raw) };
        let mut out = Vec::with_capacity(n.max(0) as usize);
        for i in 0..n {
            let name = read_cstr_bytes(|b, l| unsafe {
                sys::spout_gl_get_sender_name_at(self.raw, i, b, l)
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
        let s = read_cstr_bytes(|b, l| unsafe { sys::spout_gl_get_active_sender(self.raw, b, l) });
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
            sys::spout_gl_get_sender_info(
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
            sys::spout_gl_release_receiver(self.raw);
            if self.owns_context {
                sys::spout_gl_close_opengl(self.raw);
            }
            sys::spout_gl_destroy(self.raw);
        }
    }
}

impl core::fmt::Debug for Receiver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("gl::Receiver")
            .field("sender_name", &self.sender_name())
            .field("connected", &self.is_connected())
            .field("owns_context", &self.owns_context)
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
