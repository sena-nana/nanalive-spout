//! Safe, idiomatic Rust bindings to [Spout2](https://spout.zeal.co), the Windows
//! GPU/CPU texture-sharing system.
//!
//! Spout lets applications share video frames in real time, either as GPU
//! textures (zero-copy, via shared DirectX/OpenGL handles) or as CPU pixel
//! buffers. This crate wraps the vendored Spout2 SDK and offers two backends:
//!
//! - [`dx`] — DirectX 11, via the `spoutDX` class. Manages its own D3D11 device,
//!   so sending/receiving works with plain pixel buffers and shared D3D11
//!   textures without the caller setting up a graphics context.
//! - [`dx12`] — DirectX 12, via the `spoutDX12` class and the D3D11On12 bridge.
//!   Interoperates with D3D11 and OpenGL senders; GPU sharing wraps
//!   `ID3D12Resource` textures through D3D11On12.
//! - [`gl`] — OpenGL, via the `Spout` class. Shares GL textures directly; the
//!   CPU pixel path needs a current GL context (or the hidden one created for
//!   you — see [`gl::Sender::with_hidden_context`]).
//!
//! Each backend is gated behind a cargo feature (`dx`, `dx12`, and `gl`; `dx` and
//! `gl` are enabled by default).
//!
//! # Platform
//!
//! Spout is Windows-only and requires the MSVC toolchain. On other platforms
//! this crate compiles to an empty stub so that `cargo check` / `cargo doc`
//! still succeed. The supported architecture is `x86_64`; Windows on ARM
//! (`aarch64`) is not supported, because the vendored SSE2 SIMD sources are not
//! adapted to ARM.
//!
//! # Example
//!
//! ```no_run
//! // Works without a GPU — print the linked Spout SDK version.
//! println!("Spout SDK {}", spout2::sdk_version());
//! ```
//!
//! See the [`dx`], [`dx12`], and [`gl`] modules for sending and receiving frames, and the
//! `examples/` directory for runnable senders and receivers.
#![cfg(windows)]
#![warn(missing_docs)]

mod error;
mod util;

#[cfg(feature = "dx")]
pub mod dx;
#[cfg(feature = "dx12")]
pub mod dx12;
#[cfg(feature = "gl")]
pub mod gl;

pub use error::{Result, SpoutError};

/// The Spout SDK version this crate is built against (e.g. `"2.007.017"`).
pub use spout2_sys::SPOUT_SDK_VERSION;

/// Returns the Spout SDK version string reported by the linked native library
/// (e.g. `"2.007.017"`). This should match [`SPOUT_SDK_VERSION`].
pub fn sdk_version() -> String {
    util::read_cstr_buf(|buf, len| unsafe { spout2_sys::spout_get_sdk_version(buf, len) })
}
