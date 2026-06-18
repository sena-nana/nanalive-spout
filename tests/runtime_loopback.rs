//! Ignored runtime tests that exercise real Spout frame transfer.
//!
//! These need Windows, a working GPU/driver stack, and the relevant backend
//! feature. They are intentionally ignored so regular CI can remain headless.
#![cfg(windows)]

#[cfg(any(feature = "dx", feature = "gl"))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(any(feature = "dx", feature = "gl"))]
fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    format!("{prefix}-{}-{nanos}", std::process::id())
}

#[cfg(feature = "dx")]
#[test]
#[ignore = "requires a GPU/driver stack and real DirectX Spout transfer"]
fn dx_cpu_sender_receiver_loopback() -> spout2::Result<()> {
    let name = unique_name("spout2-rs-dx-loopback");
    let (w, h) = (16u32, 16u32);
    let pixels = vec![0x7fu8; (w * h * 4) as usize];

    let mut sender = spout2::dx::Sender::new(&name)?;
    let mut receiver = spout2::dx::Receiver::new(Some(&name))?;
    let mut received = vec![0u8; pixels.len()];

    sender.send_image(&pixels, w, h)?;

    for _ in 0..30 {
        let connected = receiver.receive_image(&mut received, w, h, false, false)?;
        if receiver.is_updated() {
            continue;
        }
        if connected && receiver.is_frame_new() {
            assert_eq!(receiver.sender_size(), (w, h));
            assert!(received.iter().any(|&b| b != 0));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    panic!("DX loopback did not receive a new frame");
}

#[cfg(feature = "gl")]
#[test]
#[ignore = "requires a GPU/driver stack and real OpenGL Spout transfer"]
fn gl_cpu_sender_receiver_loopback() -> spout2::Result<()> {
    let name = unique_name("spout2-rs-gl-loopback");
    let (w, h) = (16u32, 16u32);
    let pixels = vec![0x3fu8; (w * h * 4) as usize];

    let mut sender = spout2::gl::Sender::with_hidden_context(&name)?;
    let mut receiver = spout2::gl::Receiver::with_hidden_context(Some(&name))?;
    let mut received = Vec::new();

    sender.send_image(&pixels, w, h)?;

    for _ in 0..30 {
        let connected = receiver.receive_image(&mut received, spout2::gl::format::RGBA, false)?;
        if receiver.is_updated() {
            let (rw, rh) = receiver.sender_size();
            received.resize((rw * rh * 4) as usize, 0);
            continue;
        }
        if connected && receiver.is_frame_new() {
            assert_eq!(receiver.sender_size(), (w, h));
            assert!(received.iter().any(|&b| b != 0));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    panic!("GL loopback did not receive a new frame");
}
