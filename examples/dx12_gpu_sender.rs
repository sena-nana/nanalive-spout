//! DirectX 12 GPU texture sender example.
//!
//! Renders a solid colour into an offscreen D3D12 render target each frame and
//! publishes it through Spout via the D3D11On12 bridge. Run alongside
//! `dx12_gpu_receiver` or any Spout receiver.
//!
//! ```text
//! cargo run --example dx12_gpu_sender --features dx12
//! ```

#[cfg(all(windows, feature = "dx12"))]
fn main() -> windows::core::Result<()> {
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::Common::*;
    use windows::Win32::Graphics::Dxgi::*;
    use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
    use windows::core::Interface;

    const W: u32 = 640;
    const H: u32 = 480;

    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        let adapter: IDXGIAdapter1 = factory.EnumAdapters1(0)?;
        let mut device: Option<ID3D12Device> = None;
        D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device)?;
        let device = device.unwrap();

        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            ..Default::default()
        };
        let queue: ID3D12CommandQueue = device.CreateCommandQueue(&queue_desc)?;

        let allocator: ID3D12CommandAllocator =
            device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
        let list: ID3D12GraphicsCommandList =
            device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?;
        list.Close()?;

        let rtv_heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            NumDescriptors: 1,
            Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
            NodeMask: 0,
        };
        let rtv_heap: ID3D12DescriptorHeap = device.CreateDescriptorHeap(&rtv_heap_desc)?;
        let rtv_handle = rtv_heap.GetCPUDescriptorHandleForHeapStart();

        let mut clear_value = D3D12_CLEAR_VALUE {
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                Color: [0.2, 0.5, 0.9, 1.0],
            },
        };
        let tex_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: W as u64,
            Height: H,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
        };
        let heap_props = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            ..Default::default()
        };
        let mut texture: Option<ID3D12Resource> = None;
        device.CreateCommittedResource(
            &heap_props,
            D3D12_HEAP_FLAG_NONE,
            &tex_desc,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            Some(&clear_value),
            &mut texture,
        )?;
        let texture = texture.unwrap();
        device.CreateRenderTargetView(&texture, None, rtv_handle);

        let mut queue_ptr = queue.as_raw();
        let mut sender = spout2::dx12::Sender::with_device(
            "Rust DX12 GPU Sender",
            device.as_raw(),
            &mut queue_ptr,
        )
        .map_err(|e| {
            windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e.to_string())
        })?;

        let wrapped = sender
            .wrap_resource(
                texture.as_raw(),
                spout2::dx12::resource_state::RENDER_TARGET,
            )
            .map_err(|e| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    e.to_string(),
                )
            })?;

        let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
        let fence_event: HANDLE = CreateEventW(None, false, false, None)?;
        let mut fence_value: u64 = 1;

        println!(
            "GPU sending '{}' at {W}x{H} (SDK {}). Press Ctrl+C to stop.",
            sender.name(),
            spout2::sdk_version()
        );

        let start = Instant::now();
        let mut hue = 0.0f32;
        let mut texture_state = D3D12_RESOURCE_STATE_RENDER_TARGET;
        loop {
            hue = (hue + 0.01) % 1.0;
            let (r, g, b) = hsv_to_rgb(hue, 0.8, 0.95);
            clear_value.Anonymous.Color = [r, g, b, 1.0];

            allocator.Reset()?;
            list.Reset(&allocator, None)?;
            if texture_state != D3D12_RESOURCE_STATE_RENDER_TARGET {
                let barrier =
                    transition_barrier(&texture, texture_state, D3D12_RESOURCE_STATE_RENDER_TARGET);
                list.ResourceBarrier(&[barrier]);
            }
            list.ClearRenderTargetView(rtv_handle, &clear_value.Anonymous.Color, None);
            list.Close()?;

            let lists = [Some(list.clone().into())];
            queue.ExecuteCommandLists(&lists);

            sender.send_wrapped_resource(&wrapped).map_err(|e| {
                windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    e.to_string(),
                )
            })?;
            texture_state = D3D12_RESOURCE_STATE_PRESENT;

            queue.Signal(&fence, fence_value)?;
            if fence.GetCompletedValue() < fence_value {
                fence.SetEventOnCompletion(fence_value, fence_event)?;
                WaitForSingleObject(fence_event, INFINITE);
            }
            fence_value += 1;

            let frame = sender.frame();
            if frame % 60 == 0 {
                println!("frame {frame}, {:.1} fps", sender.fps());
            }
            if start.elapsed() > Duration::from_millis(16) {
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = fence_event;
        }
    }
}

#[cfg(all(windows, feature = "dx12"))]
unsafe fn transition_barrier(
    resource: &windows::Win32::Graphics::Direct3D12::ID3D12Resource,
    before: windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATES,
    after: windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_STATES,
) -> windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_BARRIER {
    use std::mem::ManuallyDrop;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::core::Interface;

    let raw = resource.as_raw();
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(unsafe { ID3D12Resource::from_raw(raw) })),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}

#[cfg(all(windows, feature = "dx12"))]
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

#[cfg(not(all(windows, feature = "dx12")))]
fn main() {
    eprintln!("This example requires Windows and the `dx12` feature.");
}
