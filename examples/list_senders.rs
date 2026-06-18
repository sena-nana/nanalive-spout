//! Print the Spout SDK version and the names of all running senders, then exit.
//!
//! This is the most CI-friendly example: it touches only shared memory and needs
//! no GPU, no GL context, and no peer application.
//!
//! ```text
//! cargo run --example list_senders
//! ```

#[cfg(windows)]
fn main() {
    println!("Spout SDK version: {}", spout2::sdk_version());

    let names = sender_names();
    println!("{} sender(s) currently running:", names.len());
    for name in &names {
        println!("  - {name}");
    }
}

// Enumerate via whichever backend is available (both use the same registry).
#[cfg(all(windows, feature = "gl"))]
fn sender_names() -> Vec<String> {
    spout2::gl::sender_names()
}

#[cfg(all(windows, feature = "dx", not(feature = "gl")))]
fn sender_names() -> Vec<String> {
    match spout2::dx::Receiver::new(None) {
        Ok(rx) => rx.sender_list(),
        Err(_) => Vec::new(),
    }
}

#[cfg(all(windows, not(feature = "gl"), not(feature = "dx")))]
fn sender_names() -> Vec<String> {
    Vec::new()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This example requires Windows.");
}
