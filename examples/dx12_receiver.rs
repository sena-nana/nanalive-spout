//! DirectX 12 receiver example (CPU pixel path).
//!
//! Connects to the active Spout sender (or a named one) and prints frame stats.
//! Run alongside `dx12_sender` or any Spout sender.
//!
//! ```text
//! cargo run --example dx12_receiver --features dx12
//! ```

#[cfg(all(windows, feature = "dx12"))]
fn main() -> spout2::Result<()> {
    use std::time::Duration;

    let mut receiver = spout2::dx12::Receiver::new(None)?;
    println!(
        "Receiving (SDK {}). {} sender(s) listed. Press Ctrl+C to stop.",
        spout2::sdk_version(),
        receiver.sender_count()
    );
    for name in receiver.sender_list() {
        println!("  - {name}");
    }

    let (mut w, mut h) = (640u32, 480u32);
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    loop {
        if receiver.is_updated() {
            (w, h) = receiver.sender_size();
            if w > 0 && h > 0 {
                pixels.resize((w * h * 4) as usize, 0);
                println!("Sender '{}' updated to {w}x{h}", receiver.sender_name());
            }
        }

        if receiver.receive_image(&mut pixels, w, h, false, false)? && receiver.is_frame_new() {
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

#[cfg(not(all(windows, feature = "dx12")))]
fn main() {
    eprintln!("This example requires Windows and the `dx12` feature.");
}
