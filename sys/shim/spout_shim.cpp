#include "spout_shim.h"

#include "SpoutUtils.h"

#if defined(NANAVTS_SPOUT_CPU_DX11)
#include "SpoutDX.h"
#endif

#if defined(NANAVTS_SPOUT_GPU_DX12)
#include "SpoutDX12.h"
#endif

#include <chrono>
#include <cstring>
#include <cstdint>
#include <cstdio>
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
constexpr int NANAVTS_DX12_SEND_FAILED = 0;
constexpr int NANAVTS_DX12_SEND_SENT = 1;
constexpr int NANAVTS_DX12_SEND_SKIPPED_ACCESS_TIMEOUT = 2;

using clock_type = std::chrono::steady_clock;

uint64_t elapsed_us(clock_type::time_point start)
{
    return static_cast<uint64_t>(
        std::chrono::duration_cast<std::chrono::microseconds>(clock_type::now() - start).count());
}

void init_send_result(spout_dx12_send_result_t* result)
{
    if (!result) {
        return;
    }
    result->status = NANAVTS_DX12_SEND_FAILED;
    result->frame = -1;
    result->waited_us = 0;
    result->access_wait_us = 0;
    result->submit_us = 0;
    result->flush_us = 0;
}

class nanavts_spoutDX12 : public spoutDX12 {
public:
    bool WrapDX12ResourceEx(ID3D12Resource* pDX12Resource,
                            ID3D11Resource** ppWrapped11Resource,
                            D3D12_RESOURCE_STATES initial_state,
                            D3D12_RESOURCE_STATES final_state)
    {
        if (!m_pd3d11On12Device || !pDX12Resource || !ppWrapped11Resource) {
            return false;
        }

        D3D11_RESOURCE_FLAGS d3d11_flags = {};
        if (initial_state == D3D12_RESOURCE_STATE_RENDER_TARGET) {
            d3d11_flags.BindFlags = D3D11_BIND_RENDER_TARGET;
        }

        HRESULT hr = m_pd3d11On12Device->CreateWrappedResource(
            pDX12Resource,
            &d3d11_flags,
            initial_state,
            final_state,
            IID_PPV_ARGS(ppWrapped11Resource));

        if (FAILED(hr)) {
            SpoutLogError("nanavts_spoutDX12::WrapDX12ResourceEx failed (%d 0x%.7X)", LOWORD(hr), UINT(hr));
            return false;
        }
        return true;
    }

    int SendWrappedResourceFast(ID3D11Resource* wrapped11,
                                unsigned int width,
                                unsigned int height,
                                DXGI_FORMAT format,
                                unsigned int access_timeout_ms,
                                bool collect_timing,
                                spout_dx12_send_result_t* result)
    {
        init_send_result(result);
        if (!result) {
            return 0;
        }
        if (!wrapped11 || width == 0 || height == 0 || !m_pd3d11On12Device || !m_pd3dDeviceContext11) {
            return 0;
        }
        if (!CheckSender(width, height, static_cast<DWORD>(format)) || !m_pSharedTexture) {
            return 1;
        }

        m_pd3d11On12Device->AcquireWrappedResources(&wrapped11, 1);

        uint64_t access_us = 0;
        HANDLE access_mutex = acquire_sender_access(access_timeout_ms, collect_timing, &access_us);
        result->waited_us = access_us;
        result->access_wait_us = access_us;

        if (!access_mutex) {
            m_pd3d11On12Device->ReleaseWrappedResources(&wrapped11, 1);
            auto flush_start = clock_type::now();
            m_pd3dDeviceContext11->Flush();
            result->flush_us = collect_timing ? elapsed_us(flush_start) : 0;
            result->frame = GetFrame();
            result->status = NANAVTS_DX12_SEND_SKIPPED_ACCESS_TIMEOUT;
            return 1;
        }

        auto submit_start = clock_type::now();
        m_pd3dDeviceContext11->CopyResource(m_pSharedTexture, wrapped11);
        m_pd3d11On12Device->ReleaseWrappedResources(&wrapped11, 1);
        result->submit_us = collect_timing ? elapsed_us(submit_start) : 0;

        auto flush_start = clock_type::now();
        m_pd3dDeviceContext11->Flush();
        result->flush_us = collect_timing ? elapsed_us(flush_start) : 0;

        frame.SetNewFrame();
        release_sender_access(access_mutex);
        result->frame = GetFrame();
        result->status = NANAVTS_DX12_SEND_SENT;
        return 1;
    }

private:
    HANDLE acquire_sender_access(unsigned int timeout_ms, bool collect_timing, uint64_t* waited_us)
    {
        if (waited_us) {
            *waited_us = 0;
        }
        if (!m_pSharedTexture) {
            return nullptr;
        }

        char mutex_name[512]{};
        sprintf_s(mutex_name, 512, "%s_SpoutAccessMutex", m_SenderName);
        HANDLE mutex = CreateMutexA(nullptr, false, mutex_name);
        if (!mutex || GetLastError() == ERROR_INVALID_HANDLE) {
            if (mutex) {
                CloseHandle(mutex);
            }
            return nullptr;
        }

        auto access_start = clock_type::now();
        const DWORD wait_result = WaitForSingleObject(mutex, timeout_ms);
        if (waited_us && collect_timing) {
            *waited_us = elapsed_us(access_start);
        }
        if (wait_result == WAIT_OBJECT_0) {
            return mutex;
        }

        CloseHandle(mutex);
        return nullptr;
    }

    void release_sender_access(HANDLE mutex)
    {
        if (mutex) {
            ReleaseMutex(mutex);
            CloseHandle(mutex);
        }
    }
};

inline nanavts_spoutDX12* as_dx12(spout_dx12_t* h) { return reinterpret_cast<nanavts_spoutDX12*>(h); }
}

extern "C" {

spout_dx12_t* spout_dx12_create(void) {
    try {
        return reinterpret_cast<spout_dx12_t*>(new nanavts_spoutDX12());
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
    return spout_dx12_wrap_resource_ex(
        h,
        d3d12_resource,
        initial_state,
        static_cast<unsigned int>(D3D12_RESOURCE_STATE_PRESENT),
        out_wrapped11);
}

int spout_dx12_wrap_resource_ex(spout_dx12_t* h, void* d3d12_resource,
                                unsigned int initial_state, unsigned int final_state,
                                void** out_wrapped11) {
    try {
        if (!out_wrapped11) {
            return 0;
        }
        ID3D11Resource* wrapped = nullptr;
        bool ok = as_dx12(h)->WrapDX12ResourceEx(
            reinterpret_cast<ID3D12Resource*>(d3d12_resource),
            &wrapped,
            static_cast<D3D12_RESOURCE_STATES>(initial_state),
            static_cast<D3D12_RESOURCE_STATES>(final_state));
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

int spout_dx12_send_wrapped_resource_fast(spout_dx12_t* h, void* wrapped11,
                                          unsigned int width, unsigned int height,
                                          unsigned int dxgi_format,
                                          unsigned int access_timeout_ms,
                                          unsigned int collect_timing,
                                          spout_dx12_send_result_t* out_result) {
    try {
        return as_dx12(h)->SendWrappedResourceFast(
            reinterpret_cast<ID3D11Resource*>(wrapped11),
            width,
            height,
            static_cast<DXGI_FORMAT>(dxgi_format),
            access_timeout_ms,
            collect_timing != 0,
            out_result);
    } catch (...) {
        init_send_result(out_result);
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
