/*
 * spout_shim.cpp - implementation of the flat C ABI declared in spout_shim.h.
 *
 * See spout_shim.h for the boundary conventions. Backend-specific functions are
 * guarded by SPOUT2_SHIM_DX / SPOUT2_SHIM_GL (set by build.rs).
 */
#include "spout_shim.h"

#include "SpoutUtils.h"

#if defined(SPOUT2_SHIM_DX)
#include "SpoutDX.h"
#endif

#if defined(SPOUT2_SHIM_GL)
#include "Spout.h"
#endif

#include <cstring>
#include <string>

namespace {

// Length of a NUL-terminated C string, bounded by `maxlen`.
int cstr_len(const char* s, int maxlen) {
    int n = 0;
    while (n < maxlen && s[n] != '\0') {
        ++n;
    }
    return n;
}

// Copy a std::string into a caller buffer, NUL-terminated and truncated to fit.
// Returns the number of bytes written, excluding the NUL.
int copy_string(const std::string& s, char* buf, int maxlen) {
    if (!buf || maxlen <= 0) {
        return 0;
    }
    int n = static_cast<int>(s.size());
    if (n > maxlen - 1) {
        n = maxlen - 1;
    }
    if (n > 0) {
        std::memcpy(buf, s.data(), static_cast<size_t>(n));
    }
    buf[n] = '\0';
    return n;
}

// Copy a (possibly null) C string into a caller buffer; returns bytes written.
int copy_cstr(const char* s, char* buf, int maxlen) {
    if (!buf || maxlen <= 0) {
        return 0;
    }
    if (!s) {
        buf[0] = '\0';
        return 0;
    }
    int n = cstr_len(s, maxlen - 1);
    std::memcpy(buf, s, static_cast<size_t>(n));
    buf[n] = '\0';
    return n;
}

} // namespace

extern "C" int spout_get_sdk_version(char* buf, int maxlen) {
    try {
        return copy_string(spoututils::GetSDKversion(), buf, maxlen);
    } catch (...) {
        return 0;
    }
}

/* ===================================================================== */
/* DirectX 11 backend (spoutDX)                                          */
/* ===================================================================== */
#if defined(SPOUT2_SHIM_DX)

namespace {
inline spoutDX* as_dx(spout_dx_t* h) { return reinterpret_cast<spoutDX*>(h); }
}

extern "C" {

spout_dx_t* spout_dx_create(void) {
    try {
        return reinterpret_cast<spout_dx_t*>(new spoutDX());
    } catch (...) {
        return nullptr;
    }
}

void spout_dx_destroy(spout_dx_t* h) {
    try {
        delete as_dx(h);
    } catch (...) {
    }
}

int spout_dx_open_directx11(spout_dx_t* h, void* device) {
    try {
        return as_dx(h)->OpenDirectX11(reinterpret_cast<ID3D11Device*>(device)) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

void spout_dx_close_directx11(spout_dx_t* h) {
    try {
        as_dx(h)->CloseDirectX11();
    } catch (...) {
    }
}

void* spout_dx_get_device(spout_dx_t* h) {
    try {
        return as_dx(h)->GetDX11Device();
    } catch (...) {
        return nullptr;
    }
}

void* spout_dx_get_context(spout_dx_t* h) {
    try {
        return as_dx(h)->GetDX11Context();
    } catch (...) {
        return nullptr;
    }
}

int spout_dx_set_sender_name(spout_dx_t* h, const char* name) {
    try {
        return as_dx(h)->SetSenderName(name) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

void spout_dx_set_sender_format(spout_dx_t* h, unsigned int dxgi_format) {
    try {
        as_dx(h)->SetSenderFormat(static_cast<DXGI_FORMAT>(dxgi_format));
    } catch (...) {
    }
}

void spout_dx_release_sender(spout_dx_t* h) {
    try {
        as_dx(h)->ReleaseSender();
    } catch (...) {
    }
}

int spout_dx_send_texture(spout_dx_t* h, void* texture) {
    try {
        return as_dx(h)->SendTexture(reinterpret_cast<ID3D11Texture2D*>(texture)) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_send_image(spout_dx_t* h, const unsigned char* data,
                        unsigned int width, unsigned int height, unsigned int pitch) {
    try {
        return as_dx(h)->SendImage(data, width, height, pitch) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_is_initialized(spout_dx_t* h) {
    try {
        return as_dx(h)->IsInitialized() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_get_name(spout_dx_t* h, char* buf, int maxlen) {
    try {
        return copy_cstr(as_dx(h)->GetName(), buf, maxlen);
    } catch (...) {
        return 0;
    }
}

unsigned int spout_dx_get_width(spout_dx_t* h) {
    try {
        return as_dx(h)->GetWidth();
    } catch (...) {
        return 0;
    }
}

unsigned int spout_dx_get_height(spout_dx_t* h) {
    try {
        return as_dx(h)->GetHeight();
    } catch (...) {
        return 0;
    }
}

double spout_dx_get_fps(spout_dx_t* h) {
    try {
        return as_dx(h)->GetFps();
    } catch (...) {
        return 0.0;
    }
}

long spout_dx_get_frame(spout_dx_t* h) {
    try {
        return as_dx(h)->GetFrame();
    } catch (...) {
        return 0;
    }
}

void spout_dx_hold_fps(spout_dx_t* h, int fps) {
    try {
        as_dx(h)->HoldFps(fps);
    } catch (...) {
    }
}

void spout_dx_set_receiver_name(spout_dx_t* h, const char* name) {
    try {
        as_dx(h)->SetReceiverName(name);
    } catch (...) {
    }
}

void spout_dx_release_receiver(spout_dx_t* h) {
    try {
        as_dx(h)->ReleaseReceiver();
    } catch (...) {
    }
}

int spout_dx_receive_texture(spout_dx_t* h) {
    try {
        return as_dx(h)->ReceiveTexture() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_receive_texture_into(spout_dx_t* h, void** pp_texture) {
    try {
        return as_dx(h)->ReceiveTexture(reinterpret_cast<ID3D11Texture2D**>(pp_texture)) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_receive_image(spout_dx_t* h, unsigned char* pixels,
                           unsigned int width, unsigned int height, int rgb, int invert) {
    try {
        return as_dx(h)->ReceiveImage(pixels, width, height, rgb != 0, invert != 0) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_select_sender(spout_dx_t* h, void* hwnd) {
    try {
        return as_dx(h)->SelectSender(reinterpret_cast<HWND>(hwnd)) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_is_updated(spout_dx_t* h) {
    try {
        return as_dx(h)->IsUpdated() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_is_connected(spout_dx_t* h) {
    try {
        return as_dx(h)->IsConnected() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx_is_frame_new(spout_dx_t* h) {
    try {
        return as_dx(h)->IsFrameNew() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

void* spout_dx_get_sender_texture(spout_dx_t* h) {
    try {
        return as_dx(h)->GetSenderTexture();
    } catch (...) {
        return nullptr;
    }
}

void* spout_dx_get_sender_handle(spout_dx_t* h) {
    try {
        return as_dx(h)->GetSenderHandle();
    } catch (...) {
        return nullptr;
    }
}

unsigned int spout_dx_get_sender_format(spout_dx_t* h) {
    try {
        return static_cast<unsigned int>(as_dx(h)->GetSenderFormat());
    } catch (...) {
        return 0;
    }
}

int spout_dx_get_sender_name(spout_dx_t* h, char* buf, int maxlen) {
    try {
        return copy_cstr(as_dx(h)->GetSenderName(), buf, maxlen);
    } catch (...) {
        return 0;
    }
}

unsigned int spout_dx_get_sender_width(spout_dx_t* h) {
    try {
        return as_dx(h)->GetSenderWidth();
    } catch (...) {
        return 0;
    }
}

unsigned int spout_dx_get_sender_height(spout_dx_t* h) {
    try {
        return as_dx(h)->GetSenderHeight();
    } catch (...) {
        return 0;
    }
}

double spout_dx_get_sender_fps(spout_dx_t* h) {
    try {
        return as_dx(h)->GetSenderFps();
    } catch (...) {
        return 0.0;
    }
}

long spout_dx_get_sender_frame(spout_dx_t* h) {
    try {
        return as_dx(h)->GetSenderFrame();
    } catch (...) {
        return 0;
    }
}

int spout_dx_get_sender_count(spout_dx_t* h) {
    try {
        return as_dx(h)->GetSenderCount();
    } catch (...) {
        return 0;
    }
}

int spout_dx_get_sender_name_at(spout_dx_t* h, int index, char* buf, int maxlen) {
    try {
        if (!buf || maxlen <= 0) {
            return 0;
        }
        buf[0] = '\0';
        if (!as_dx(h)->GetSender(index, buf, maxlen)) {
            buf[0] = '\0';
            return 0;
        }
        return cstr_len(buf, maxlen);
    } catch (...) {
        return 0;
    }
}

int spout_dx_get_active_sender(spout_dx_t* h, char* buf, int maxlen) {
    try {
        if (!buf || maxlen <= 0) {
            return 0;
        }
        buf[0] = '\0';
        if (!as_dx(h)->GetActiveSender(buf)) {
            buf[0] = '\0';
            return 0;
        }
        return cstr_len(buf, maxlen);
    } catch (...) {
        return 0;
    }
}

int spout_dx_get_sender_info(spout_dx_t* h, const char* name, unsigned int* width,
                             unsigned int* height, void** share_handle, unsigned int* format) {
    try {
        unsigned int w = 0, ht = 0;
        HANDLE handle = nullptr;
        DWORD fmt = 0;
        bool ok = as_dx(h)->GetSenderInfo(name, w, ht, handle, fmt);
        if (width) *width = w;
        if (height) *height = ht;
        if (share_handle) *share_handle = handle;
        if (format) *format = static_cast<unsigned int>(fmt);
        return ok ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

} // extern "C"

#endif /* SPOUT2_SHIM_DX */

/* ===================================================================== */
/* OpenGL backend (Spout)                                                */
/* ===================================================================== */
#if defined(SPOUT2_SHIM_GL)

namespace {
inline Spout* as_gl(spout_gl_t* h) { return reinterpret_cast<Spout*>(h); }
}

extern "C" {

spout_gl_t* spout_gl_create(void) {
    try {
        return reinterpret_cast<spout_gl_t*>(new Spout());
    } catch (...) {
        return nullptr;
    }
}

void spout_gl_destroy(spout_gl_t* h) {
    try {
        delete as_gl(h);
    } catch (...) {
    }
}

int spout_gl_create_opengl(spout_gl_t* h, void* hwnd) {
    try {
        return as_gl(h)->CreateOpenGL(reinterpret_cast<HWND>(hwnd)) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_close_opengl(spout_gl_t* h) {
    try {
        return as_gl(h)->CloseOpenGL() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

void spout_gl_set_sender_name(spout_gl_t* h, const char* name) {
    try {
        as_gl(h)->SetSenderName(name);
    } catch (...) {
    }
}

void spout_gl_set_sender_format(spout_gl_t* h, unsigned int dw_format) {
    try {
        as_gl(h)->SetSenderFormat(static_cast<DWORD>(dw_format));
    } catch (...) {
    }
}

void spout_gl_release_sender(spout_gl_t* h) {
    try {
        as_gl(h)->ReleaseSender();
    } catch (...) {
    }
}

int spout_gl_send_fbo(spout_gl_t* h, unsigned int fbo,
                      unsigned int width, unsigned int height, int invert) {
    try {
        return as_gl(h)->SendFbo(fbo, width, height, invert != 0) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_send_texture(spout_gl_t* h, unsigned int tex_id, unsigned int tex_target,
                          unsigned int width, unsigned int height, int invert,
                          unsigned int host_fbo) {
    try {
        return as_gl(h)->SendTexture(tex_id, tex_target, width, height, invert != 0, host_fbo)
                   ? 1
                   : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_send_image(spout_gl_t* h, const unsigned char* pixels,
                        unsigned int width, unsigned int height, unsigned int gl_format,
                        int invert, unsigned int host_fbo) {
    try {
        return as_gl(h)->SendImage(pixels, width, height, gl_format, invert != 0, host_fbo) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_is_initialized(spout_gl_t* h) {
    try {
        return as_gl(h)->IsInitialized() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_get_name(spout_gl_t* h, char* buf, int maxlen) {
    try {
        return copy_cstr(as_gl(h)->GetName(), buf, maxlen);
    } catch (...) {
        return 0;
    }
}

unsigned int spout_gl_get_width(spout_gl_t* h) {
    try {
        return as_gl(h)->GetWidth();
    } catch (...) {
        return 0;
    }
}

unsigned int spout_gl_get_height(spout_gl_t* h) {
    try {
        return as_gl(h)->GetHeight();
    } catch (...) {
        return 0;
    }
}

double spout_gl_get_fps(spout_gl_t* h) {
    try {
        return as_gl(h)->GetFps();
    } catch (...) {
        return 0.0;
    }
}

long spout_gl_get_frame(spout_gl_t* h) {
    try {
        return as_gl(h)->GetFrame();
    } catch (...) {
        return 0;
    }
}

void* spout_gl_get_handle(spout_gl_t* h) {
    try {
        return as_gl(h)->GetHandle();
    } catch (...) {
        return nullptr;
    }
}

void spout_gl_hold_fps(spout_gl_t* h, int fps) {
    try {
        as_gl(h)->HoldFps(fps);
    } catch (...) {
    }
}

void spout_gl_set_receiver_name(spout_gl_t* h, const char* name) {
    try {
        as_gl(h)->SetReceiverName(name);
    } catch (...) {
    }
}

void spout_gl_release_receiver(spout_gl_t* h) {
    try {
        as_gl(h)->ReleaseReceiver();
    } catch (...) {
    }
}

int spout_gl_receive(spout_gl_t* h) {
    try {
        return as_gl(h)->ReceiveTexture() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_receive_texture(spout_gl_t* h, unsigned int tex_id, unsigned int tex_target,
                             int invert, unsigned int host_fbo) {
    try {
        return as_gl(h)->ReceiveTexture(tex_id, tex_target, invert != 0, host_fbo) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_receive_image(spout_gl_t* h, unsigned char* pixels, unsigned int gl_format,
                           int invert, unsigned int host_fbo) {
    try {
        return as_gl(h)->ReceiveImage(pixels, gl_format, invert != 0, host_fbo) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_is_updated(spout_gl_t* h) {
    try {
        return as_gl(h)->IsUpdated() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_is_connected(spout_gl_t* h) {
    try {
        return as_gl(h)->IsConnected() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_is_frame_new(spout_gl_t* h) {
    try {
        return as_gl(h)->IsFrameNew() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_get_sender_name(spout_gl_t* h, char* buf, int maxlen) {
    try {
        return copy_cstr(as_gl(h)->GetSenderName(), buf, maxlen);
    } catch (...) {
        return 0;
    }
}

unsigned int spout_gl_get_sender_width(spout_gl_t* h) {
    try {
        return as_gl(h)->GetSenderWidth();
    } catch (...) {
        return 0;
    }
}

unsigned int spout_gl_get_sender_height(spout_gl_t* h) {
    try {
        return as_gl(h)->GetSenderHeight();
    } catch (...) {
        return 0;
    }
}

unsigned int spout_gl_get_sender_format(spout_gl_t* h) {
    try {
        return static_cast<unsigned int>(as_gl(h)->GetSenderFormat());
    } catch (...) {
        return 0;
    }
}

double spout_gl_get_sender_fps(spout_gl_t* h) {
    try {
        return as_gl(h)->GetSenderFps();
    } catch (...) {
        return 0.0;
    }
}

long spout_gl_get_sender_frame(spout_gl_t* h) {
    try {
        return as_gl(h)->GetSenderFrame();
    } catch (...) {
        return 0;
    }
}

void* spout_gl_get_sender_handle(spout_gl_t* h) {
    try {
        return as_gl(h)->GetSenderHandle();
    } catch (...) {
        return nullptr;
    }
}

int spout_gl_select_sender(spout_gl_t* h, void* hwnd) {
    try {
        return as_gl(h)->SelectSender(reinterpret_cast<HWND>(hwnd)) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_gl_get_sender_count(spout_gl_t* h) {
    try {
        return as_gl(h)->GetSenderCount();
    } catch (...) {
        return 0;
    }
}

int spout_gl_get_sender_name_at(spout_gl_t* h, int index, char* buf, int maxlen) {
    try {
        if (!buf || maxlen <= 0) {
            return 0;
        }
        buf[0] = '\0';
        if (!as_gl(h)->GetSender(index, buf, maxlen)) {
            buf[0] = '\0';
            return 0;
        }
        return cstr_len(buf, maxlen);
    } catch (...) {
        return 0;
    }
}

int spout_gl_get_active_sender(spout_gl_t* h, char* buf, int maxlen) {
    try {
        if (!buf || maxlen <= 0) {
            return 0;
        }
        buf[0] = '\0';
        if (!as_gl(h)->GetActiveSender(buf)) {
            buf[0] = '\0';
            return 0;
        }
        return cstr_len(buf, maxlen);
    } catch (...) {
        return 0;
    }
}

int spout_gl_get_sender_info(spout_gl_t* h, const char* name, unsigned int* width,
                             unsigned int* height, void** share_handle, unsigned int* format) {
    try {
        unsigned int w = 0, ht = 0;
        HANDLE handle = nullptr;
        DWORD fmt = 0;
        bool ok = as_gl(h)->GetSenderInfo(name, w, ht, handle, fmt);
        if (width) *width = w;
        if (height) *height = ht;
        if (share_handle) *share_handle = handle;
        if (format) *format = static_cast<unsigned int>(fmt);
        return ok ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

} // extern "C"

#endif /* SPOUT2_SHIM_GL */
