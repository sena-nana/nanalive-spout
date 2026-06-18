//! OpenGL receiver example.
//!
//! Connects to the active Spout sender (using a hidden OpenGL context) and
//! prints frame info as it arrives. Run `gl_sender` (or any Spout sender) in
//! another terminal first. Requires a GPU.
//!
//! ```text
//! cargo run --example gl_receiver
//! ```

#[cfg(all(windows, feature = "gl"))]
fn main() -> spout2::Result<()> {
    use std::time::Duration;

    let mut receiver = spout2::gl::Receiver::with_hidden_context(None)?;
    println!("Available senders: {:?}", spout2::gl::sender_names());
    println!("Waiting for a sender... Press Ctrl+C to stop.");

    let mut pixels: Vec<u8> = Vec::new();
    loop {
        let connected = receiver.receive_image(&mut pixels, spout2::gl::format::RGBA, false)?;

        if receiver.is_updated() {
            // Sender appeared or changed size: resize and refill next iteration.
            let (w, h) = receiver.sender_size();
            pixels.resize((w * h * 4) as usize, 0);
            println!("connected to '{}' at {w}x{h}", receiver.sender_name());
            continue;
        }

        if connected && receiver.is_frame_new() {
            println!(
                "'{}' {}x{} frame {} {:.1} fps",
                receiver.sender_name(),
                receiver.sender_width(),
                receiver.sender_height(),
                receiver.sender_frame(),
                receiver.sender_fps()
            );
        }

        std::thread::sleep(Duration::from_millis(16));
    }
}

#[cfg(not(all(windows, feature = "gl")))]
fn main() {
    eprintln!("This example requires Windows and the `gl` feature.");
}
