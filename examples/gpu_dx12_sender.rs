//! Compile-only DX12 sender API example.

use core::ffi::c_void;
use nanavts_spout::{
    GpuDx12ExperimentalSender, GpuDx12PublishOptions, ID3D12CommandQueue, ID3D12Device,
    SpoutFormat, SpoutFrameRef, SpoutSenderBackend,
};

fn main() {
    eprintln!(
        "Provide an existing ID3D12Device, ID3D12CommandQueue, and ID3D12Resource \
         to publish with this API example."
    );
}

/// Publish one caller-owned D3D12 texture resource through Spout's D3D11On12 path.
///
/// # Safety
///
/// `device`, `queue`, and `resource` must be valid D3D12 COM pointers from the
/// same device/queue family. The resource must be in `initial_state` before the
/// call and may be used as `final_state` after the call returns.
pub unsafe fn publish_existing_dx12_resource(
    device: *mut ID3D12Device,
    queue: *mut ID3D12CommandQueue,
    resource: *mut c_void,
    width: u32,
    height: u32,
    initial_state: u32,
    final_state: u32,
) -> nanavts_spout::Result<nanavts_spout::SpoutPublishReport> {
    let mut sender = unsafe {
        GpuDx12ExperimentalSender::with_d3d12_device_and_queue("NanaVTS DX12", device, queue)?
    };
    sender.resize_or_recreate(width, height, SpoutFormat::R8G8B8A8_UNORM)?;
    sender.set_publish_options(GpuDx12PublishOptions {
        access_timeout_ms: 1,
        collect_timing: true,
    });
    sender.publish_report(SpoutFrameRef::Dx12Resource {
        resource,
        initial_state,
        final_state,
    })
}
