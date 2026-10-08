//! Shared-texture Spout sender example.
//!
//! A stand-in producer (a second D3D11 device on the same adapter) exports
//! three NT-handle shared BGRA8 textures and a shared fence the way NanaUI's
//! DX12 `NativeExportPool` does: frame `k` is ready at fence value `2k + 1`
//! and released at `2k + 2`. `GpuDx11SharedSender` waits for ready on the GPU,
//! copies the slot into Spout's surface and signals release. Open "NanaLive
//! shared DX11" in a Spout receiver (OBS Spout2 Capture) to see it.
//!
//! `cargo run --example gpu_dx11_shared_sender --no-default-features --features gpu-dx11-shared -- [seconds]`

#[cfg(all(windows, feature = "gpu-dx11-shared"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};

    use nanalive_spout::{GpuDx11SharedOptions, GpuDx11SharedSender, SharedTextureFrame};
    use windows::Win32::Foundation::{GENERIC_ALL, HMODULE};
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_FENCE_FLAG_SHARED, D3D11_RESOURCE_MISC_SHARED, D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
        D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice,
        ID3D11Device5, ID3D11DeviceContext4, ID3D11Fence, ID3D11RenderTargetView, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGIResource1};
    use windows::core::{Interface, PCWSTR};

    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(10);
    let (width, height) = (1280u32, 720u32);

    unsafe {
        // The producer.
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
        )?;
        let device = device.unwrap();
        let context = context.unwrap();
        let device5: ID3D11Device5 = device.cast()?;
        let context4: ID3D11DeviceContext4 = context.cast()?;
        let luid = device
            .cast::<IDXGIDevice>()?
            .GetAdapter()?
            .GetDesc()?
            .AdapterLuid;
        let adapter_luid = (i64::from(luid.HighPart) << 32) | i64::from(luid.LowPart);
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
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
        let mut slots: Vec<(ID3D11Texture2D, ID3D11RenderTargetView)> = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..3 {
            let mut texture = None;
            device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.unwrap();
            let mut view = None;
            device.CreateRenderTargetView(&texture, None, Some(&mut view))?;
            let handle = texture.cast::<IDXGIResource1>()?.CreateSharedHandle(
                None,
                GENERIC_ALL.0,
                PCWSTR::null(),
            )?;
            handles.push(handle.0);
            slots.push((texture, view.unwrap()));
        }
        let fence: ID3D11Fence = {
            let mut fence: Option<ID3D11Fence> = None;
            device5.CreateFence(0, D3D11_FENCE_FLAG_SHARED, &mut fence)?;
            fence.unwrap()
        };
        let fence_handle = fence.CreateSharedHandle(None, GENERIC_ALL.0, PCWSTR::null())?;

        // The consumer.
        let mut sender = GpuDx11SharedSender::new("NanaLive shared DX11", adapter_luid)?;
        let start = Instant::now();
        let mut frame = 0u64;
        let mut deferred = 0u64;
        while start.elapsed() < Duration::from_secs(seconds) {
            let ready = 2 * frame + 1;
            // The producer only writes a slot again once the previous frame
            // was released; it never waits on the CPU for that.
            if frame > 0 && fence.GetCompletedValue() < ready - 1 {
                deferred += 1;
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            let slot = (frame % 3) as usize;
            let t = start.elapsed().as_secs_f32();
            let color = [0.5 + 0.5 * t.sin(), 0.3, 0.5 + 0.5 * (t * 0.7).cos(), 1.0];
            context.ClearRenderTargetView(&slots[slot].1, &color);
            context4.Signal(&fence, ready)?;
            context.Flush();
            let report = sender.send(
                &SharedTextureFrame {
                    pool_generation: 1,
                    adapter_luid,
                    width,
                    height,
                    textures: &handles,
                    fence: fence_handle.0,
                    slot,
                    ready_value: ready,
                    release_value: ready + 1,
                },
                GpuDx11SharedOptions::default(),
            )?;
            frame += 1;
            if frame % 120 == 0 {
                let status = sender.status();
                println!(
                    "frame {frame}: {:?}, sent {}, skipped {}, {:.1} fps, deferred {deferred}",
                    report.status,
                    status.sent_count,
                    status.skipped_access_timeout_count,
                    status.fps.unwrap_or_default()
                );
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        let status = sender.status();
        println!(
            "done: {frame} frames, sent {}, skipped {}, failed {}, released without copy {}, fence {}",
            status.sent_count,
            status.skipped_access_timeout_count,
            status.failed_count,
            sender.released_without_copy(),
            fence.GetCompletedValue()
        );
        for handle in handles.into_iter().chain([fence_handle.0]) {
            let _ =
                windows::Win32::Foundation::CloseHandle(windows::Win32::Foundation::HANDLE(handle));
        }
    }
    Ok(())
}

#[cfg(not(all(windows, feature = "gpu-dx11-shared")))]
fn main() {
    eprintln!("This example requires Windows and the `gpu-dx11-shared` feature.");
}
