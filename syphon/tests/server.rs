#![cfg(target_os = "macos")]

use nanalive_syphon::{Error, Published, Server};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice, MTLOrigin,
    MTLPixelFormat, MTLRegion, MTLSize, MTLStorageMode, MTLTexture, MTLTextureDescriptor,
    MTLTextureUsage,
};
use std::ptr::NonNull;
use std::time::{Duration, Instant};

const WIDTH: u32 = 8;
const HEIGHT: u32 = 4;

fn device() -> Retained<ProtocolObject<dyn MTLDevice>> {
    MTLCreateSystemDefaultDevice().expect("a Metal device")
}

fn texture(
    device: &ProtocolObject<dyn MTLDevice>,
    format: MTLPixelFormat,
    pixels: &[u8],
) -> Retained<ProtocolObject<dyn MTLTexture>> {
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            format,
            WIDTH as usize,
            HEIGHT as usize,
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::ShaderRead | MTLTextureUsage::RenderTarget);
    descriptor.setStorageMode(MTLStorageMode::Managed);
    let texture = device
        .newTextureWithDescriptor(&descriptor)
        .expect("texture");
    let region = MTLRegion {
        origin: MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width: WIDTH as usize,
            height: HEIGHT as usize,
            depth: 1,
        },
    };
    unsafe {
        texture.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
            region,
            0,
            NonNull::new(pixels.as_ptr().cast_mut().cast()).unwrap(),
            WIDTH as usize * 4,
        );
    }
    texture
}

/// Each row a different colour, so a vertical flip changes the bytes.
fn gradient() -> Vec<u8> {
    (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).flat_map(move |x| [x as u8 * 10, y as u8 * 60, 200, 128]))
        .collect()
}

fn wait_for_frame(server: &Server) -> ([u32; 2], Vec<u8>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(frame) = server.read_published_frame() {
            return frame;
        }
        assert!(Instant::now() < deadline, "no frame was published");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_published_frame_reaches_the_shared_surface_unchanged_and_top_row_first() {
    let device = device();
    let queue = device.newCommandQueue().expect("queue");
    let pixels = gradient();
    let source = texture(&device, MTLPixelFormat::BGRA8Unorm, &pixels);
    let server = Server::new("NanaLive Test", &device).expect("server");
    assert_eq!(server.name(), "NanaLive Test");
    assert!(server.read_published_frame().is_none());

    let command_buffer = queue.commandBuffer().expect("command buffer");
    assert_eq!(
        server.publish(&source, &command_buffer, [WIDTH, HEIGHT]),
        Ok(Published::Sent)
    );
    command_buffer.commit();
    command_buffer.waitUntilCompleted();

    let (size, published) = wait_for_frame(&server);
    assert_eq!(size, [WIDTH, HEIGHT]);
    assert_eq!(published, pixels);
}

#[test]
fn a_second_frame_waits_for_the_first_to_complete() {
    let device = device();
    let queue = device.newCommandQueue().expect("queue");
    let source = texture(&device, MTLPixelFormat::BGRA8Unorm, &gradient());
    let server = Server::new("NanaLive Busy", &device).expect("server");
    let first = queue.commandBuffer().expect("command buffer");
    assert_eq!(
        server.publish(&source, &first, [WIDTH, HEIGHT]),
        Ok(Published::Sent)
    );
    let second = queue.commandBuffer().expect("command buffer");
    assert_eq!(
        server.publish(&source, &second, [WIDTH, HEIGHT]),
        Ok(Published::Busy)
    );
    first.commit();
    second.commit();
    first.waitUntilCompleted();
    wait_for_frame(&server);
    let third = queue.commandBuffer().expect("command buffer");
    assert_eq!(
        server.publish(&source, &third, [WIDTH, HEIGHT]),
        Ok(Published::Sent)
    );
    third.commit();
}

#[test]
fn textures_off_the_blit_path_and_bad_sizes_are_rejected() {
    let device = device();
    let queue = device.newCommandQueue().expect("queue");
    let server = Server::new("NanaLive Reject", &device).expect("server");
    let srgb = texture(&device, MTLPixelFormat::BGRA8Unorm_sRGB, &gradient());
    let plain = texture(&device, MTLPixelFormat::BGRA8Unorm, &gradient());
    let command_buffer = queue.commandBuffer().expect("command buffer");
    for (texture, size) in [
        (&srgb, [WIDTH, HEIGHT]),
        (&plain, [0, HEIGHT]),
        (&plain, [WIDTH + 1, HEIGHT]),
    ] {
        assert_eq!(
            server.publish(texture, &command_buffer, size),
            Err(Error::TextureNotBlittable)
        );
    }
}

#[test]
fn servers_survive_repeated_start_publish_and_stop() {
    let device = device();
    let queue = device.newCommandQueue().expect("queue");
    let source = texture(&device, MTLPixelFormat::BGRA8Unorm, &gradient());
    for round in 0..50 {
        let server = Server::new(&format!("NanaLive Cycle {round}"), &device).expect("server");
        assert!(!server.has_clients());
        let command_buffer = queue.commandBuffer().expect("command buffer");
        assert_eq!(
            server.publish(&source, &command_buffer, [WIDTH, HEIGHT]),
            Ok(Published::Sent)
        );
        command_buffer.commit();
        // Dropped with the frame possibly still in flight: the stop is
        // deferred to its completion handler.
        drop(server);
    }
}
