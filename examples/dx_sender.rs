//! DirectX 11 sender example.
//!
//! Publishes a moving RGBA gradient as a Spout sender named "Rust DX Sender".
//! Run alongside `dx_receiver` (or any Spout receiver, e.g. the SpoutDX demo) to
//! see the frames. Requires a GPU.
//!
//! ```text
//! cargo run --example dx_sender
//! ```

#[cfg(all(windows, feature = "dx"))]
fn main() -> spout2::Result<()> {
    use std::time::{Duration, Instant};

    let (w, h) = (640u32, 480u32);
    let mut sender = spout2::dx::Sender::new("Rust DX Sender")?;
    println!(
        "Sending '{}' at {w}x{h} (SDK {}). Press Ctrl+C to stop.",
        sender.name(),
        spout2::sdk_version()
    );

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let start = Instant::now();
    loop {
        let t = start.elapsed().as_secs_f32();
        let shift = (t * 60.0) as u32;
        for y in 0..h {
            let row = (y * w * 4) as usize;
            for x in 0..w {
                let i = row + (x * 4) as usize;
                pixels[i] = ((x + shift) & 0xff) as u8; // B
                pixels[i + 1] = ((y + shift / 2) & 0xff) as u8; // G
                pixels[i + 2] = (shift & 0xff) as u8; // R
                pixels[i + 3] = 255; // A
            }
        }
        sender.send_image(&pixels, w, h)?;

        let frame = sender.frame();
        if frame % 60 == 0 {
            println!("frame {frame}, {:.1} fps", sender.fps());
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(not(all(windows, feature = "dx")))]
fn main() {
    eprintln!("This example requires Windows and the `dx` feature.");
}
