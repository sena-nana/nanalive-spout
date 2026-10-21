# nanalive-syphon

`nanalive-syphon` is a NanaLive-focused Syphon output crate for macOS. It
exposes one Metal server and nothing else: no clients, no server directory,
no OpenGL.

```rust,no_run
# #[cfg(target_os = "macos")]
# fn send(
#     device: &objc2::runtime::ProtocolObject<dyn nanalive_syphon::MTLDevice>,
#     texture: &objc2::runtime::ProtocolObject<dyn nanalive_syphon::MTLTexture>,
#     command_buffer: &objc2::runtime::ProtocolObject<dyn nanalive_syphon::MTLCommandBuffer>,
# ) -> Result<(), nanalive_syphon::Error> {
use nanalive_syphon::{Published, Server};

let server = Server::new("Stage", device)?;
if server.has_clients() {
    if server.publish(texture, command_buffer, [1920, 1080])? == Published::Sent {
        // commit `command_buffer` after the work that wrote `texture`
    }
}
# Ok(())
# }
```

## Contract

- **Format.** Only textures on Syphon's blit path are accepted: `BGRA8Unorm`,
  one sample, not framebuffer-only. Clients receive the bytes unchanged and
  untagged; OBS's *Syphon Client* treats them as sRGB with straight alpha, so
  convert premultiplied or linear content in your own pass first.
- **Orientation.** The texture's top row is the shared surface's first row
  (`flipped: NO`, the blit path).
- **One frame in flight.** `publish` returns `Published::Busy` until the
  previous command buffer completes. Dropping a `Server` with a frame in flight
  defers the stop to that frame's completion; both guard upstream's unlocked
  IOSurface resize and release.
- **Threads.** Create, publish and drop from any thread. Discovery replies go
  through `NSDistributedNotificationCenter` on the main thread, so clients
  that start after the server only find it while the main run loop runs.
- **Sandboxing.** Syphon uses global IOSurfaces, which an App Sandbox forbids.

## Build

The Metal server half of [Syphon-Framework](https://github.com/Syphon/Syphon-Framework)
is a git submodule in `vendor/`, pinned to `f4761677` (after PR #103, which OBS
32's Metal renderer needs). `build.rs` compiles it statically with a flat C shim
(`shim/`), so there is no `Syphon.framework` to embed or sign. It replaces the
prefix header and the redraw renderer, whose shaders live in a framework bundle,
and prefixes every Objective-C class with `NanaLive` so this copy never clashes
with another Syphon in the same process. Off macOS the crate is an empty stub.

```text
git submodule update --init --recursive
cargo test -p nanalive-syphon
```

Binary distributions must reproduce `THIRD_PARTY_NOTICES.txt`. Syphon's licence
also forbids using the Syphon Project's name to endorse the product.
