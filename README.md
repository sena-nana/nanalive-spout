# spout2-rs

Safe Rust bindings to [**Spout2**](https://spout.zeal.co), the Windows system for
sharing video frames between applications in real time — either as zero-copy GPU
textures or as CPU pixel buffers.

The crate wraps the vendored Spout 2.007.017 C++ SDK and exposes **three backends**:

| Module | Backend | Best for |
| ------ | ------- | -------- |
| [`dx`] | DirectX 11 (`spoutDX`) | The simplest path. Manages its own D3D11 device, so CPU pixel send/receive works with no graphics setup. Also shares `ID3D11Texture2D` handles. Great for wgpu/D3D apps and bridging to OBS, Resolume, TouchDesigner, etc. |
| [`dx12`] | DirectX 12 (`spoutDX12`) | D3D12 apps via the **D3D11On12** bridge. Interoperates with D3D11 and OpenGL senders. CPU pixels work out of the box; GPU sharing wraps `ID3D12Resource` textures through D3D11On12. Opt-in via the `dx12` feature. |
| [`gl`] | OpenGL (`Spout`) | Sharing OpenGL textures directly (glow/glium). The CPU path uses a hidden GL context Spout creates for you. |

All backends support sending **and** receiving, CPU pixel buffers **and** GPU
textures (where applicable), plus sender discovery, frame-rate/size/frame getters, and
new-frame/connection detection.

**Note:** Spout's inter-process protocol uses D3D11 shared textures. The `dx12`
backend bridges D3D12 resources through Microsoft's D3D11On12 layer — it is not
native D3D12 shared-handle sharing.

Sender names are returned as `String` for ergonomic Rust use. Spout stores names
as ANSI bytes, so byte-oriented APIs such as `dx::Receiver::sender_list_bytes`
and `gl::sender_names_bytes` are also available when interoperating with
non-UTF-8 names from other applications.

## Platform

- **Windows only.** Spout shares textures through Windows/DirectX/OpenGL; the
  crate compiles to an empty stub on other platforms so `cargo check`/`cargo doc`
  still succeed.
- **MSVC toolchain required** (`*-pc-windows-msvc`). The vendored C++ uses
  MSVC-only facilities; the GNU toolchain is not supported.
- **`x86_64` only.** Windows on ARM (`aarch64-pc-windows-msvc`) is not supported:
  Spout's SSE2 SIMD sources would need the upstream `sse2neon` shim, which this
  crate's build does not configure. The build fails fast on `aarch64` with a
  clear message.
- A GPU is required to actually share frames. Sender enumeration and the version
  query work without one.

## Adding the dependency

```toml
[dependencies]
spout2-rs = "0.1"
```

The crate name you import is `spout2`:

```rust
use spout2::dx;
use spout2::dx12;
use spout2::gl;
```

Default features enable `dx` and `gl`. To pick backends:

```toml
spout2-rs = { version = "0.1", default-features = false, features = ["dx12"] }
```

## Quick start

### Send CPU pixels (DirectX 11)

```rust,no_run
fn main() -> spout2::Result<()> {
    let (w, h) = (640u32, 480u32);
    let mut sender = spout2::dx::Sender::new("My Rust Sender")?;
    let pixels = vec![0u8; (w * h * 4) as usize]; // BGRA / RGBA, 8 bits per channel
    loop {
        // ... fill `pixels` ...
        sender.send_image(&pixels, w, h)?;
    }
}
```

### Receive CPU pixels (DirectX 11)

```rust,no_run
fn main() -> spout2::Result<()> {
    let mut receiver = spout2::dx::Receiver::new(None)?; // None = active sender
    let (mut w, mut h) = (256u32, 256u32);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    loop {
        let connected = receiver.receive_image(&mut pixels, w, h, false, false)?;
        if receiver.is_updated() {
            (w, h) = receiver.sender_size();
            pixels.resize((w * h * 4) as usize, 0);
            continue; // next call fills the resized buffer
        }
        if connected && receiver.is_frame_new() {
            // ... use `pixels` ...
        }
    }
}
```

### OpenGL

```rust,no_run
fn main() -> spout2::Result<()> {
    // Spout creates a hidden GL context so no windowing crate is needed.
    let mut sender = spout2::gl::Sender::with_hidden_context("My GL Sender")?;
    let (w, h) = (640u32, 480u32);
    let pixels = vec![0u8; (w * h * 4) as usize];
    sender.send_image(&pixels, w, h)?;
    Ok(())
}
```

If your application already has a current GL context, construct with the
`unsafe` [`gl::Sender::in_current_context`] / [`gl::Receiver::in_current_context`]
instead, and use the `unsafe` texture/FBO methods for zero-copy GPU sharing.

### GPU texture sharing

- **DirectX 11:** [`dx::Sender::send_texture`] / [`dx::Receiver::sender_texture_ptr`]
  work with `ID3D11Texture2D*` (as `*mut c_void`). Use
  [`dx::Sender::with_device`] to share your own D3D11 device.
- **DirectX 12:** senders use [`dx12::Sender::with_device`] to share their D3D12
  device, then [`dx12::Sender::wrap_resource`] once per render target and
  [`dx12::Sender::send_wrapped_resource`] each frame. Receivers construct with
  [`dx12::Receiver::with_device`] (so the receiving textures share that device —
  required for the D3D11On12 wrap), then call [`dx12::Receiver::create_texture`]
  after [`dx12::Receiver::is_updated`] and [`dx12::Receiver::receive_resource`]
  each frame. Requires the `dx12` feature.
- **OpenGL:** [`gl::Sender::send_texture`] / [`gl::Receiver::receive_texture`]
  take `GLuint` texture and FBO names.

These methods are `unsafe`: you must uphold the documented invariants (valid
handles on the right device/context, received pointers are valid only for the
current frame).

## Examples

```text
cargo run --example list_senders   # prints the SDK version and running senders (no GPU)
cargo run --example dx_sender       # publish a moving gradient (DirectX 11)
cargo run --example dx_receiver     # receive and report frames (DirectX 11)
cargo run --example dx12_sender --features dx12       # CPU pixels (DirectX 12)
cargo run --example dx12_receiver --features dx12     # CPU receive (DirectX 12)
cargo run --example dx12_gpu_sender --features dx12   # GPU texture send (D3D11On12)
cargo run --example dx12_gpu_receiver --features dx12 # GPU texture receive
cargo run --example gl_sender       # publish a moving gradient (OpenGL)
cargo run --example gl_receiver     # receive and report frames (OpenGL)
```

Run a sender and a receiver in two terminals to see frames flow.

## Building from source

The Spout SDK is a git submodule. After cloning:

```text
git submodule update --init --recursive
cargo build
```

The build script compiles the SDK and a small C++ shim with the
[`cc`](https://crates.io/crates/cc) crate and links statically — there is **no
runtime DLL** to ship and **no bindgen/libclang** dependency.

## How it works

`spout2-sys` compiles the vendored Spout C++ sources together with a hand-written
flat `extern "C"` shim (`sys/shim/`), so Rust binds a stable plain-C surface
instead of a C++ vtable with STL return types. `spout2-rs` is the safe wrapper:
RAII handles, `Result`-based errors, buffer-size validation, and `unsafe` only
where GPU resource invariants must be upheld by the caller.

## Versioning

Bundled Spout SDK: **2.007.017** (see [`SPOUT_SDK_VERSION`] and the runtime
[`sdk_version`]).

Maintainer release steps are documented in [RELEASING.md](RELEASING.md). Publish
`spout2-sys` before `spout2-rs` so the top-level crate can resolve its matching
native dependency from crates.io.

## License

`spout2-rs` is licensed under the BSD 2-Clause license to match the bundled
Spout2 SDK (also BSD 2-Clause, © Lynn Jarvis). See [LICENSE](LICENSE) and
[`sys/vendor/Spout2`](sys/vendor/Spout2) for details.

[`dx`]: https://docs.rs/spout2/latest/spout2/dx/
[`dx12`]: https://docs.rs/spout2/latest/spout2/dx12/
[`gl`]: https://docs.rs/spout2/latest/spout2/gl/
[`dx12::Sender::with_device`]: https://docs.rs/spout2/latest/spout2/dx12/struct.Sender.html#method.with_device
[`dx12::Sender::wrap_resource`]: https://docs.rs/spout2/latest/spout2/dx12/struct.Sender.html#method.wrap_resource
[`dx12::Receiver::with_device`]: https://docs.rs/spout2/latest/spout2/dx12/struct.Receiver.html#method.with_device
[`dx12::Sender::send_wrapped_resource`]: https://docs.rs/spout2/latest/spout2/dx12/struct.Sender.html#method.send_wrapped_resource
[`dx12::Receiver::create_texture`]: https://docs.rs/spout2/latest/spout2/dx12/struct.Receiver.html#method.create_texture
[`dx12::Receiver::receive_resource`]: https://docs.rs/spout2/latest/spout2/dx12/struct.Receiver.html#method.receive_resource
[`dx12::Receiver::is_updated`]: https://docs.rs/spout2/latest/spout2/dx12/struct.Receiver.html#method.is_updated
[`dx::Sender::send_texture`]: https://docs.rs/spout2/latest/spout2/dx/struct.Sender.html#method.send_texture
[`dx::Sender::with_device`]: https://docs.rs/spout2/latest/spout2/dx/struct.Sender.html#method.with_device
[`dx::Receiver::sender_texture_ptr`]: https://docs.rs/spout2/latest/spout2/dx/struct.Receiver.html#method.sender_texture_ptr
[`gl::Sender::send_texture`]: https://docs.rs/spout2/latest/spout2/gl/struct.Sender.html#method.send_texture
[`gl::Sender::in_current_context`]: https://docs.rs/spout2/latest/spout2/gl/struct.Sender.html#method.in_current_context
[`gl::Receiver::in_current_context`]: https://docs.rs/spout2/latest/spout2/gl/struct.Receiver.html#method.in_current_context
[`gl::Receiver::receive_texture`]: https://docs.rs/spout2/latest/spout2/gl/struct.Receiver.html#method.receive_texture
[`SPOUT_SDK_VERSION`]: https://docs.rs/spout2/latest/spout2/constant.SPOUT_SDK_VERSION.html
[`sdk_version`]: https://docs.rs/spout2/latest/spout2/fn.sdk_version.html
