//! DirectX 12 GPU texture receiver example.
//!
//! Connects to the active Spout sender and receives frames into a D3D12 texture
//! via the D3D11On12 bridge. The receiver shares this app's D3D12 device and
//! command queue (via [`spout2::dx12::Receiver::with_device`]) so the textures
//! created here live on the same device Spout wraps through D3D11On12 — that
//! device match is required for the wrap to succeed. Run alongside
//! `dx12_gpu_sender` or any Spout sender.
//!
//! ```text
//! cargo run --example dx12_gpu_receiver --features dx12
//! ```

#[cfg(all(windows, feature = "dx12"))]
fn main() -> windows::core::Result<()> {
    use core::ffi::c_void;
    use std::time::Duration;
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::*;
    use windows::core::Interface;

    fn spout_err(e: spout2::SpoutError) -> windows::core::Error {
        windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e.to_string())
    }

    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        let adapter: IDXGIAdapter1 = factory.EnumAdapters1(0)?;
        let mut device: Option<ID3D12Device> = None;
        D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device)?;
        let device = device.unwrap();

        // Spout's D3D11On12 bridge submits the wrapped-resource copy onto a queue
        // from our device, so it needs one created here.
        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            ..Default::default()
        };
        let queue: ID3D12CommandQueue = device.CreateCommandQueue(&queue_desc)?;

        // Share our device + queue with the receiver so the textures we create
        // below can be wrapped by Spout's D3D11On12 device (same-device requirement).
        let mut queue_ptr = queue.as_raw();
        let mut receiver =
            spout2::dx12::Receiver::with_device(None, device.as_raw(), &mut queue_ptr)
                .map_err(spout_err)?;
        println!(
            "GPU receiving (SDK {}). {} sender(s). Waiting for connection...",
            spout2::sdk_version(),
            receiver.sender_count()
        );

        let mut received_texture: Option<ID3D12Resource> = None;

        loop {
            // When the sender appears or changes size/format, (re)create the
            // receiving texture on our device.
            if receiver.is_updated() {
                received_texture = None;

                let (w, h) = receiver.sender_size();
                let format = receiver.sender_format();
                if w > 0 && h > 0 {
                    let raw = receiver
                        .create_texture(
                            device.as_raw(),
                            w,
                            h,
                            spout2::dx12::resource_state::COPY_DEST,
                            format,
                        )
                        .map_err(spout_err)?;
                    received_texture = Some(ID3D12Resource::from_raw(raw));
                    println!(
                        "Created receiving texture for '{}' ({w}x{h}, format {format})",
                        receiver.sender_name()
                    );
                }
            }

            // Drive the connection every frame. Before the first texture exists the
            // slot is null, which only connects (no copy); afterwards Spout copies
            // the sender's frame into our texture.
            let mut raw_ptr: *mut c_void = match &received_texture {
                Some(tex) => tex.as_raw(),
                None => core::ptr::null_mut(),
            };
            let connected = receiver.receive_resource(&mut raw_ptr).map_err(spout_err)?;

            if connected && received_texture.is_some() && receiver.is_frame_new() {
                let frame = receiver.sender_frame();
                if frame % 60 == 0 {
                    println!(
                        "frame {frame}, {:.1} fps from '{}'",
                        receiver.sender_fps(),
                        receiver.sender_name()
                    );
                }
            }

            std::thread::sleep(Duration::from_millis(16));
        }
    }
}

#[cfg(not(all(windows, feature = "dx12")))]
fn main() {
    eprintln!("This example requires Windows and the `dx12` feature.");
}
