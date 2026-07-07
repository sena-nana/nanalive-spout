# nanavts-spout

`nanavts-spout` is a NanaVTS-focused Spout output crate. It intentionally exposes
only sender backends needed by NanaVTS instead of the full Spout2 SDK surface.

## Backends

| Feature | Backend | Status |
| ------- | ------- | ------ |
| `cpu-dx11` | CPU pixel sender through Spout DirectX 11 | Default |
| `gpu-dx12-experimental` | D3D12 resource sender through Spout's D3D11On12 bridge | Opt-in experimental |

No receiver API, OpenGL backend, sender discovery, sender selection UI, `winit`,
or `glutin` dependency is exposed.

## Platform

- Spout output is available on Windows x86_64 with the MSVC toolchain.
- Non-Windows targets compile as stubs and report `UnsupportedPlatform` or an
  unavailable backend status.
- Windows GNU and Windows ARM targets fail early with clear build errors.
- The vendored Spout2 SDK is a git submodule. If native sources are missing, run:

```text
git submodule update --init --recursive
```

## Quick Start

```rust,no_run
use nanavts_spout::{
    CpuDx11Sender, SpoutFormat, SpoutFrameRef, SpoutSenderBackend,
};

fn main() -> nanavts_spout::Result<()> {
    let (width, height) = (1280, 720);
    let pixels = vec![0u8; (width * height * 4) as usize];

    let mut sender = CpuDx11Sender::new("NanaVTS")?;
    sender.resize_or_recreate(width, height, SpoutFormat::default())?;
    sender.publish(SpoutFrameRef::CpuPixels {
        pixels: &pixels,
        width,
        height,
        pitch_bytes: None,
    })?;

    Ok(())
}
```

## Experimental DX12

The DX12 backend is only for GPU resource output experiments. It uses Spout's
D3D11On12 bridge and must be constructed from an existing D3D12 device and
command queue:

```rust,no_run
# use nanavts_spout::{GpuDx12ExperimentalSender, ID3D12CommandQueue, ID3D12Device};
# unsafe fn demo(device: *mut ID3D12Device, queue: *mut ID3D12CommandQueue) -> nanavts_spout::Result<()> {
let sender = GpuDx12ExperimentalSender::with_d3d12_device_and_queue(
    "NanaVTS DX12",
    device,
    queue,
)?;
# let _ = sender;
# Ok(())
# }
```

Enable it with:

```toml
nanavts-spout = { path = "...", default-features = false, features = ["gpu-dx12-experimental"] }
```

## Performance Probe

The `spout_perf` example compares the CPU DX11 sender path and the experimental
GPU DX12 sender path on Windows:

```text
cargo run --example spout_perf --features gpu-dx12-experimental -- --mode both --frames 600 --warmup 60
```

It reports per-frame publish-call latency (`publish(...)` entry to return),
process CPU usage, process GPU Engine usage, Spout FPS, and frame counters.
GPU usage depends on the Windows GPU Engine performance counters; on systems
where those counters are unavailable, the tool reports `n/a` with the reason
instead of inventing a value.

Useful options:

```text
--mode both|cpu|gpu-dx12
--width 1280 --height 720
--frames 600 --warmup 60
--name nanavts-spout-perf
--csv
```

## Build Checks

```text
cargo check --workspace --no-default-features
cargo check --workspace --features cpu-dx11
cargo check --workspace --no-default-features --features gpu-dx12-experimental
```

## License

`nanavts-spout` is licensed under the BSD 2-Clause license to match the bundled
Spout2 SDK. See [LICENSE](LICENSE) and `sys/vendor/Spout2` for details.
