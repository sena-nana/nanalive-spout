//! Declarations for `shim/nanalive_syphon.h`.

use std::ffi::{c_char, c_void};

pub(crate) const NL_SYPHON_OK: i32 = 0;
pub(crate) const NL_SYPHON_BUSY: i32 = 1;
pub(crate) const NL_SYPHON_NOT_BLITTABLE: i32 = 2;
pub(crate) const NL_SYPHON_STOPPED: i32 = 3;

unsafe extern "C" {
    pub(crate) fn nl_syphon_server_create(
        name_utf8: *const c_char,
        mtl_device: *mut c_void,
        error: *mut c_char,
        error_len: usize,
    ) -> *mut c_void;
    pub(crate) fn nl_syphon_server_publish(
        server: *mut c_void,
        mtl_texture: *mut c_void,
        mtl_command_buffer: *mut c_void,
        width: u32,
        height: u32,
    ) -> i32;
    pub(crate) fn nl_syphon_server_has_clients(server: *mut c_void) -> bool;
    pub(crate) fn nl_syphon_server_copy_frame(
        server: *mut c_void,
        dst: *mut u8,
        dst_len: usize,
        width: *mut u32,
        height: *mut u32,
    ) -> bool;
    pub(crate) fn nl_syphon_server_destroy(server: *mut c_void);
}
