//! Shared-texture sender against a stand-in producer on the same adapter.

#![cfg(all(windows, feature = "gpu-dx11-shared"))]

use core::ffi::c_void;
use std::time::{Duration, Instant};

use nanalive_spout::{
    GpuDx11SharedOptions, GpuDx11SharedSender, SharedTextureFrame, SpoutOutputError,
    SpoutPublishStatus,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE, HMODULE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_FENCE_FLAG_SHARED, D3D11_RESOURCE_MISC_SHARED, D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device,
    ID3D11Device5, ID3D11DeviceContext, ID3D11DeviceContext4, ID3D11Fence, ID3D11RenderTargetView,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGIResource1};
use windows::core::{Interface, PCWSTR};

const SIZE: (u32, u32) = (64, 32);

/// Three shared BGRA8 textures and a shared fence, like an exporting pool.
struct Producer {
    context: ID3D11DeviceContext,
    context4: ID3D11DeviceContext4,
    views: Vec<ID3D11RenderTargetView>,
    handles: Vec<*mut c_void>,
    fence: ID3D11Fence,
    fence_handle: *mut c_void,
    luid: i64,
    _device: ID3D11Device,
}

impl Producer {
    fn new() -> Self {
        unsafe {
            let (mut device, mut context) = (None, None);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .unwrap();
            let device: ID3D11Device = device.unwrap();
            let context: ID3D11DeviceContext = context.unwrap();
            let luid = device
                .cast::<IDXGIDevice>()
                .unwrap()
                .GetAdapter()
                .unwrap()
                .GetDesc()
                .unwrap()
                .AdapterLuid;
            let desc = D3D11_TEXTURE2D_DESC {
                Width: SIZE.0,
                Height: SIZE.1,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: (D3D11_RESOURCE_MISC_SHARED.0 | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0)
                    as u32,
            };
            let mut views = Vec::new();
            let mut handles = Vec::new();
            for _ in 0..3 {
                let mut texture = None;
                device
                    .CreateTexture2D(&desc, None, Some(&mut texture))
                    .unwrap();
                let texture = texture.unwrap();
                let mut view = None;
                device
                    .CreateRenderTargetView(&texture, None, Some(&mut view))
                    .unwrap();
                views.push(view.unwrap());
                handles.push(
                    texture
                        .cast::<IDXGIResource1>()
                        .unwrap()
                        .CreateSharedHandle(None, GENERIC_ALL.0, PCWSTR::null())
                        .unwrap()
                        .0,
                );
            }
            let mut fence: Option<ID3D11Fence> = None;
            device
                .cast::<ID3D11Device5>()
                .unwrap()
                .CreateFence(0, D3D11_FENCE_FLAG_SHARED, &mut fence)
                .unwrap();
            let fence = fence.unwrap();
            let fence_handle = fence
                .CreateSharedHandle(None, GENERIC_ALL.0, PCWSTR::null())
                .unwrap()
                .0;
            Self {
                context4: context.cast().unwrap(),
                context,
                views,
                handles,
                fence,
                fence_handle,
                luid: (i64::from(luid.HighPart) << 32) | i64::from(luid.LowPart),
                _device: device,
            }
        }
    }

    /// Write frame `k` into its slot and signal it ready.
    fn produce(&self, k: u64) -> SharedTextureFrame<'_> {
        let slot = (k % 3) as usize;
        unsafe {
            self.context
                .ClearRenderTargetView(&self.views[slot], &[0.2, 0.4, 0.6, 1.0]);
            self.context4.Signal(&self.fence, 2 * k + 1).unwrap();
            self.context.Flush();
        }
        SharedTextureFrame {
            pool_generation: 7,
            adapter_luid: self.luid,
            width: SIZE.0,
            height: SIZE.1,
            textures: &self.handles,
            fence: self.fence_handle,
            slot,
            ready_value: 2 * k + 1,
            release_value: 2 * k + 2,
        }
    }

    fn wait_completed(&self, value: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while unsafe { self.fence.GetCompletedValue() } < value {
            assert!(Instant::now() < deadline, "fence never reached {value}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Producer {
    fn drop(&mut self) {
        for handle in self.handles.iter().chain([&self.fence_handle]) {
            unsafe {
                let _ = CloseHandle(HANDLE(*handle));
            }
        }
    }
}

#[test]
fn every_frame_is_sent_and_released() {
    let producer = Producer::new();
    let mut sender = GpuDx11SharedSender::new("nanalive-spout shared test", producer.luid).unwrap();
    for k in 0..8 {
        let frame = producer.produce(k);
        let report = unsafe { sender.send(&frame, GpuDx11SharedOptions::default()) }.unwrap();
        assert_ne!(report.status, SpoutPublishStatus::Failed);
        producer.wait_completed(2 * k + 2);
    }
    assert_eq!(sender.pool_opens(), 1);
    let status = sender.status();
    assert_eq!(
        status.sent_count + status.skipped_access_timeout_count,
        8,
        "{status:?}"
    );
}

#[test]
fn a_new_pool_generation_is_reopened_and_other_adapters_are_refused() {
    let producer = Producer::new();
    let mut sender = GpuDx11SharedSender::new("nanalive-spout reopen test", producer.luid).unwrap();
    let frame = producer.produce(0);
    unsafe { sender.send(&frame, GpuDx11SharedOptions::default()) }.unwrap();
    producer.wait_completed(2);
    let next = SharedTextureFrame {
        pool_generation: 8,
        ..producer.produce(1)
    };
    unsafe { sender.send(&next, GpuDx11SharedOptions::default()) }.unwrap();
    producer.wait_completed(4);
    assert_eq!(sender.pool_opens(), 2);
    let foreign = SharedTextureFrame {
        adapter_luid: producer.luid ^ 1,
        ..producer.produce(2)
    };
    assert_eq!(
        unsafe { sender.send(&foreign, GpuDx11SharedOptions::default()) }.unwrap_err(),
        SpoutOutputError::DeviceInteropUnavailable
    );
}
