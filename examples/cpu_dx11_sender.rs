//! CPU DX11 sender example for the NANALIVE Spout output API.

#[cfg(all(windows, feature = "cpu-dx11"))]
fn main() -> NANALIVE_spout::Result<()> {
    use NANALIVE_spout::{
        CpuDx11Sender, SpoutFormat, SpoutFrameRef, SpoutSenderBackend, sdk_version,
    };
    use std::time::{Duration, Instant};

    let (w, h) = (640u32, 480u32);
    let mut sender = CpuDx11Sender::new("NANALIVE CPU DX11")?;
    sender.resize_or_recreate(w, h, SpoutFormat::default())?;

    println!(
        "Publishing NANALIVE CPU DX11 Spout frames (SDK {sdk}).",
        sdk = sdk_version()
    );

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let start = Instant::now();
    loop {
        let shift = (start.elapsed().as_secs_f32() * 60.0) as u32;
        for y in 0..h {
            let row = (y * w * 4) as usize;
            for x in 0..w {
                let i = row + (x * 4) as usize;
                pixels[i] = ((x + shift) & 0xff) as u8;
                pixels[i + 1] = ((y + shift / 2) & 0xff) as u8;
                pixels[i + 2] = (shift & 0xff) as u8;
                pixels[i + 3] = 255;
            }
        }
        sender.publish(SpoutFrameRef::CpuPixels {
            pixels: &pixels,
            width: w,
            height: h,
            pitch_bytes: None,
        })?;

        let status = sender.status();
        if status.frame.unwrap_or_default() % 60 == 0 {
            println!(
                "frame {}, {:.1} fps",
                status.frame.unwrap_or_default(),
                status.fps.unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(not(all(windows, feature = "cpu-dx11")))]
fn main() {
    eprintln!("This example requires Windows and the `cpu-dx11` feature.");
}
