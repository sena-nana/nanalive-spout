//! Internal raw FFI bindings for NanaVTS Spout sender output.
//!
//! This crate compiles the vendored Spout2 C++ sources together with a narrow
//! `extern "C"` shim. The public NanaVTS-facing API lives in `nanavts-spout`.
#![allow(non_camel_case_types)]

/// The Spout SDK version this crate is vendored against.
pub const SPOUT_SDK_VERSION: &str = "2.007.017";

#[cfg(windows)]
mod ffi {
    use core::ffi::{c_char, c_int};
    #[cfg(any(feature = "cpu-dx11", feature = "gpu-dx12-experimental"))]
    use core::ffi::{c_double, c_long, c_uint, c_void};

    unsafe extern "C" {
        /// Copy the linked SDK version into `buf`; returns bytes written
        /// excluding the NUL terminator, or 0 on failure.
        pub fn spout_get_sdk_version(buf: *mut c_char, maxlen: c_int) -> c_int;
    }

    #[cfg(feature = "cpu-dx11")]
    #[repr(C)]
    pub struct spout_dx_t {
        _private: [u8; 0],
    }

    #[cfg(feature = "cpu-dx11")]
    unsafe extern "C" {
        pub fn spout_dx_create() -> *mut spout_dx_t;
        pub fn spout_dx_destroy(h: *mut spout_dx_t);
        pub fn spout_dx_open_directx11(h: *mut spout_dx_t, device: *mut c_void) -> c_int;
        pub fn spout_dx_get_device(h: *mut spout_dx_t) -> *mut c_void;
        pub fn spout_dx_set_sender_name(h: *mut spout_dx_t, name: *const c_char) -> c_int;
        pub fn spout_dx_set_sender_format(h: *mut spout_dx_t, dxgi_format: c_uint);
        pub fn spout_dx_release_sender(h: *mut spout_dx_t);
        pub fn spout_dx_send_image(
            h: *mut spout_dx_t,
            data: *const u8,
            width: c_uint,
            height: c_uint,
            pitch: c_uint,
        ) -> c_int;
        pub fn spout_dx_is_initialized(h: *mut spout_dx_t) -> c_int;
        pub fn spout_dx_get_width(h: *mut spout_dx_t) -> c_uint;
        pub fn spout_dx_get_height(h: *mut spout_dx_t) -> c_uint;
        pub fn spout_dx_get_fps(h: *mut spout_dx_t) -> c_double;
        pub fn spout_dx_get_frame(h: *mut spout_dx_t) -> c_long;
    }

    #[cfg(feature = "gpu-dx12-experimental")]
    #[repr(C)]
    pub struct spout_dx12_t {
        _private: [u8; 0],
    }

    #[cfg(feature = "gpu-dx12-experimental")]
    unsafe extern "C" {
        pub fn spout_dx12_create() -> *mut spout_dx12_t;
        pub fn spout_dx12_destroy(h: *mut spout_dx12_t);
        pub fn spout_dx12_open_directx12(
            h: *mut spout_dx12_t,
            device: *mut c_void,
            command_queue: *mut *mut c_void,
        ) -> c_int;
        pub fn spout_dx12_get_d3d12_device(h: *mut spout_dx12_t) -> *mut c_void;
        pub fn spout_dx12_wrap_resource(
            h: *mut spout_dx12_t,
            d3d12_resource: *mut c_void,
            initial_state: c_uint,
            out_wrapped11: *mut *mut c_void,
        ) -> c_int;
        pub fn spout_dx12_send_wrapped_resource(
            h: *mut spout_dx12_t,
            wrapped11: *mut c_void,
        ) -> c_int;
        pub fn spout_dx12_release_wrapped_resource(wrapped11: *mut c_void);
        pub fn spout_dx12_set_sender_name(h: *mut spout_dx12_t, name: *const c_char) -> c_int;
        pub fn spout_dx12_set_sender_format(h: *mut spout_dx12_t, dxgi_format: c_uint);
        pub fn spout_dx12_release_sender(h: *mut spout_dx12_t);
        pub fn spout_dx12_is_initialized(h: *mut spout_dx12_t) -> c_int;
        pub fn spout_dx12_get_width(h: *mut spout_dx12_t) -> c_uint;
        pub fn spout_dx12_get_height(h: *mut spout_dx12_t) -> c_uint;
        pub fn spout_dx12_get_fps(h: *mut spout_dx12_t) -> c_double;
        pub fn spout_dx12_get_frame(h: *mut spout_dx12_t) -> c_long;
    }
}

#[cfg(windows)]
pub use ffi::*;
