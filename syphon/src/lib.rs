//! A Syphon Metal server for NanaLive's macOS output.
//!
//! [`Server`] publishes the top-left region of a caller-owned Metal texture to
//! every Syphon client on the machine (OBS's *Syphon Client* source among
//! them). Frames go through Syphon's GPU blit into a global IOSurface; nothing
//! is read back to the CPU.
//!
//! Only textures that take Syphon's blit path are accepted: `BGRA8Unorm`,
//! single-sample, not framebuffer-only. Convert anything else in your own pass
//! first. Clients read the bytes as-is, without a colour-space tag.
//!
//! Server discovery uses `NSDistributedNotificationCenter`, which delivers on
//! the main thread: a server can be created and fed from any thread, but
//! clients that start after it only find it while the main run loop runs.
//!
//! Off macOS the crate compiles to a stub whose constructor always fails with
//! [`Error::UnsupportedPlatform`].

#![warn(missing_docs)]

use std::fmt;

#[cfg(target_os = "macos")]
mod sys;

#[cfg(target_os = "macos")]
pub use objc2_metal::{MTLCommandBuffer, MTLDevice, MTLTexture};

/// What [`Server::publish`] did with a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Published {
    /// A blit was encoded; commit the command buffer to deliver it.
    Sent,
    /// The previous frame has not completed on the GPU, so nothing was
    /// encoded. The command buffer may still be committed (or dropped).
    Busy,
}

/// Why the server could not be created or a frame could not be published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Syphon is only available on macOS.
    UnsupportedPlatform,
    /// `SyphonMetalServer` did not start.
    ServerCreateFailed(String),
    /// The texture is not `BGRA8Unorm`, is multisampled or framebuffer-only,
    /// or the size is zero or larger than the texture.
    TextureNotBlittable,
    /// The server was already stopped.
    Stopped,
    /// Syphon raised an Objective-C exception while encoding.
    PublishFailed,
}

impl Error {
    /// A stable snake_case code for logs and user-facing error mapping.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "syphon_unsupported",
            Self::ServerCreateFailed(_) => "syphon_server_create_failed",
            Self::TextureNotBlittable => "syphon_texture_not_blittable",
            Self::Stopped => "syphon_server_stopped",
            Self::PublishFailed => "syphon_publish_failed",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ServerCreateFailed(reason) => write!(f, "{}: {reason}", self.code()),
            _ => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for Error {}

/// A running Syphon server. Dropping it stops the server and removes it from
/// clients' lists; a frame still in flight finishes first.
pub struct Server {
    #[cfg(target_os = "macos")]
    raw: std::ptr::NonNull<std::ffi::c_void>,
    name: String,
}

// SAFETY: `SyphonMetalServer` is documented as safe to use across threads,
// and the shim serialises its own state with `@synchronized`.
unsafe impl Send for Server {}
unsafe impl Sync for Server {}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// The human-readable name clients show next to the application name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Error, Published, Server};
    use objc2::runtime::ProtocolObject;
    use objc2_metal::{MTLCommandBuffer, MTLDevice, MTLTexture};
    use std::ffi::{CString, c_char, c_void};
    use std::ptr::NonNull;

    fn object_ptr<T: ?Sized>(object: &ProtocolObject<T>) -> *mut c_void {
        std::ptr::from_ref(object).cast::<c_void>().cast_mut()
    }

    impl Server {
        /// Starts a server named `name` for textures on `device`.
        pub fn new(name: &str, device: &ProtocolObject<dyn MTLDevice>) -> Result<Self, Error> {
            let c_name = CString::new(name.replace('\0', ""))
                .map_err(|error| Error::ServerCreateFailed(error.to_string()))?;
            let mut reason = [0 as c_char; 256];
            // SAFETY: the pointers are valid for the call; the shim retains
            // what it keeps.
            let raw = unsafe {
                super::sys::nl_syphon_server_create(
                    c_name.as_ptr(),
                    object_ptr(device),
                    reason.as_mut_ptr(),
                    reason.len(),
                )
            };
            let Some(raw) = NonNull::new(raw) else {
                // SAFETY: the shim always NUL-terminates `reason`.
                let reason = unsafe { std::ffi::CStr::from_ptr(reason.as_ptr()) };
                return Err(Error::ServerCreateFailed(
                    reason.to_string_lossy().into_owned(),
                ));
            };
            Ok(Self {
                raw,
                name: name.to_owned(),
            })
        }

        /// Whether any client is attached. Cheap enough to poll every frame.
        pub fn has_clients(&self) -> bool {
            // SAFETY: `raw` is a live handle.
            unsafe { super::sys::nl_syphon_server_has_clients(self.raw.as_ptr()) }
        }

        /// Encodes a blit of the top-left `size` of `texture` onto
        /// `command_buffer`. On [`Published::Sent`] the caller must commit the
        /// buffer, after any work that writes `texture`; clients see the frame
        /// when it completes.
        pub fn publish(
            &self,
            texture: &ProtocolObject<dyn MTLTexture>,
            command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
            size: [u32; 2],
        ) -> Result<Published, Error> {
            // SAFETY: all three objects are live for the call; the shim
            // retains the command buffer through its completed handler.
            let status = unsafe {
                super::sys::nl_syphon_server_publish(
                    self.raw.as_ptr(),
                    object_ptr(texture),
                    object_ptr(command_buffer),
                    size[0],
                    size[1],
                )
            };
            match status {
                super::sys::NL_SYPHON_OK => Ok(Published::Sent),
                super::sys::NL_SYPHON_BUSY => Ok(Published::Busy),
                super::sys::NL_SYPHON_NOT_BLITTABLE => Err(Error::TextureNotBlittable),
                super::sys::NL_SYPHON_STOPPED => Err(Error::Stopped),
                _ => Err(Error::PublishFailed),
            }
        }

        /// Reads back the last completed frame as tightly packed BGRA rows,
        /// top row first, with its size. For tests and diagnostics: it maps
        /// the shared surface on the CPU.
        pub fn read_published_frame(&self) -> Option<([u32; 2], Vec<u8>)> {
            let (mut width, mut height) = (0u32, 0u32);
            // SAFETY: a null destination only queries the size.
            unsafe {
                super::sys::nl_syphon_server_copy_frame(
                    self.raw.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                    &mut width,
                    &mut height,
                );
            }
            if width == 0 || height == 0 {
                return None;
            }
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            // SAFETY: `pixels` holds `width * height * 4` bytes.
            let copied = unsafe {
                super::sys::nl_syphon_server_copy_frame(
                    self.raw.as_ptr(),
                    pixels.as_mut_ptr(),
                    pixels.len(),
                    &mut width,
                    &mut height,
                )
            };
            copied.then_some(([width, height], pixels))
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            // SAFETY: `raw` is owned and released exactly once here.
            unsafe { super::sys::nl_syphon_server_destroy(self.raw.as_ptr()) }
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl Server {
    /// Always fails off macOS.
    pub fn new(name: &str) -> Result<Self, Error> {
        let _ = name;
        Err(Error::UnsupportedPlatform)
    }

    /// Never true off macOS.
    pub fn has_clients(&self) -> bool {
        false
    }
}
