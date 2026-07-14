//! Ignored runtime tests that exercise real Spout sender publication.
#![cfg(all(windows, feature = "cpu-dx11"))]

use nanalive_spout::{CpuDx11Sender, SpoutFrameRef, SpoutSenderBackend};

#[test]
#[ignore = "requires a Windows GPU/driver stack and a real Spout sender publish"]
fn cpu_dx11_sender_publishes_one_frame() -> nanalive_spout::Result<()> {
    let (w, h) = (16u32, 16u32);
    let pixels = vec![0x7fu8; (w * h * 4) as usize];
    let mut sender = CpuDx11Sender::new("nanalive-runtime-test")?;

    sender.publish(SpoutFrameRef::CpuPixels {
        pixels: &pixels,
        width: w,
        height: h,
        pitch_bytes: None,
    })?;

    let status = sender.status();
    assert!(status.active);
    assert_eq!(status.width, Some(w));
    assert_eq!(status.height, Some(h));
    assert!(status.frame.unwrap_or_default() >= 0);
    Ok(())
}
