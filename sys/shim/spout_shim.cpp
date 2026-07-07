#include "spout_shim.h"

#include "SpoutUtils.h"

#if defined(NANAVTS_SPOUT_CPU_DX11)
#include "SpoutDX.h"
#endif

#if defined(NANAVTS_SPOUT_GPU_DX12)
#include "SpoutDX12.h"
#endif

#include <cstring>
#include <string>

namespace {

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

} // namespace

extern "C" int spout_get_sdk_version(char* buf, int maxlen) {
    try {
        return copy_string(spoututils::GetSDKversion(), buf, maxlen);
    } catch (...) {
        return 0;
    }
}

#if defined(NANAVTS_SPOUT_CPU_DX11)

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

void* spout_dx_get_device(spout_dx_t* h) {
    try {
        return as_dx(h)->GetDX11Device();
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

} // extern "C"

#endif

#if defined(NANAVTS_SPOUT_GPU_DX12)

namespace {
inline spoutDX12* as_dx12(spout_dx12_t* h) { return reinterpret_cast<spoutDX12*>(h); }
}

extern "C" {

spout_dx12_t* spout_dx12_create(void) {
    try {
        return reinterpret_cast<spout_dx12_t*>(new spoutDX12());
    } catch (...) {
        return nullptr;
    }
}

void spout_dx12_destroy(spout_dx12_t* h) {
    try {
        delete as_dx12(h);
    } catch (...) {
    }
}

int spout_dx12_open_directx12(spout_dx12_t* h, void* device, void** command_queue) {
    try {
        ID3D12Device* d3d12_device = reinterpret_cast<ID3D12Device*>(device);
        if (d3d12_device && !as_dx12(h)->GetD3D12device()) {
            d3d12_device->AddRef();
        }
        IUnknown** pp_queue = reinterpret_cast<IUnknown**>(command_queue);
        return as_dx12(h)->OpenDirectX12(d3d12_device, pp_queue) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

void* spout_dx12_get_d3d12_device(spout_dx12_t* h) {
    try {
        return as_dx12(h)->GetD3D12device();
    } catch (...) {
        return nullptr;
    }
}

int spout_dx12_wrap_resource(spout_dx12_t* h, void* d3d12_resource,
                             unsigned int initial_state, void** out_wrapped11) {
    try {
        if (!out_wrapped11) {
            return 0;
        }
        ID3D11Resource* wrapped = nullptr;
        bool ok = as_dx12(h)->WrapDX12Resource(
            reinterpret_cast<ID3D12Resource*>(d3d12_resource),
            &wrapped,
            static_cast<D3D12_RESOURCE_STATES>(initial_state));
        *out_wrapped11 = wrapped;
        return ok ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

int spout_dx12_send_wrapped_resource(spout_dx12_t* h, void* wrapped11) {
    try {
        return as_dx12(h)->SendDX11Resource(reinterpret_cast<ID3D11Resource*>(wrapped11)) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

void spout_dx12_release_wrapped_resource(void* wrapped11) {
    try {
        ID3D11Resource* resource = reinterpret_cast<ID3D11Resource*>(wrapped11);
        if (resource) {
            resource->Release();
        }
    } catch (...) {
    }
}

int spout_dx12_set_sender_name(spout_dx12_t* h, const char* name) {
    try {
        return as_dx12(h)->SetSenderName(name) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

void spout_dx12_set_sender_format(spout_dx12_t* h, unsigned int dxgi_format) {
    try {
        as_dx12(h)->SetSenderFormat(static_cast<DXGI_FORMAT>(dxgi_format));
    } catch (...) {
    }
}

void spout_dx12_release_sender(spout_dx12_t* h) {
    try {
        as_dx12(h)->ReleaseSender();
    } catch (...) {
    }
}

int spout_dx12_is_initialized(spout_dx12_t* h) {
    try {
        return as_dx12(h)->IsInitialized() ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

unsigned int spout_dx12_get_width(spout_dx12_t* h) {
    try {
        return as_dx12(h)->GetWidth();
    } catch (...) {
        return 0;
    }
}

unsigned int spout_dx12_get_height(spout_dx12_t* h) {
    try {
        return as_dx12(h)->GetHeight();
    } catch (...) {
        return 0;
    }
}

double spout_dx12_get_fps(spout_dx12_t* h) {
    try {
        return as_dx12(h)->GetFps();
    } catch (...) {
        return 0.0;
    }
}

long spout_dx12_get_frame(spout_dx12_t* h) {
    try {
        return as_dx12(h)->GetFrame();
    } catch (...) {
        return 0;
    }
}

} // extern "C"

#endif
