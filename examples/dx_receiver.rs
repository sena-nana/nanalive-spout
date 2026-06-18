//! DirectX 11 receiver example.
//!
//! Connects to the active Spout sender and prints frame info as it arrives. Run
//! `dx_sender` (or any Spout sender) in another terminal first. Requires a GPU.
//!
//! ```text
//! cargo run --example dx_receiver
//! ```

#[cfg(all(windows, feature = "dx"))]
fn main() -> spout2::Result<()> {
    use std::time::Duration;

    let mut receiver = spout2::dx::Receiver::new(None)?;
    println!("Available senders: {:?}", receiver.sender_list());
    println!("Waiting for a sender... Press Ctrl+C to stop.");

    // Start with a small buffer; we resize to the sender's size on connect.
    let (mut w, mut h) = (256u32, 256u32);
    let mut pixels = vec![0u8; (w * h * 4) as usize];

    loop {
        let connected = receiver.receive_image(&mut pixels, w, h, false, false)?;

        if receiver.is_updated() {
            // Sender appeared or changed size: resize and refill next iteration.
            (w, h) = receiver.sender_size();
            pixels.resize((w * h * 4) as usize, 0);
            println!("connected to '{}' at {w}x{h}", receiver.sender_name());
            continue;
        }

        if connected {
            if receiver.is_frame_new() {
                let row0: u64 = pixels
                    .iter()
                    .take((w * 4) as usize)
                    .map(|&b| b as u64)
                    .sum();
                println!(
                    "'{}' {w}x{h} frame {} {:.1} fps (row0 checksum {row0})",
                    receiver.sender_name(),
                    receiver.sender_frame(),
                    receiver.sender_fps()
                );
            }
        } else {
            println!(
                "no sender connected; available: {:?}",
                receiver.sender_list()
            );
        }

        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(not(all(windows, feature = "dx")))]
fn main() {
    eprintln!("This example requires Windows and the `dx` feature.");
}
