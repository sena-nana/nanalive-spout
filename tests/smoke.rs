//! GPU-free smoke tests. These run on any Windows machine without a Spout peer
//! application and without a discrete GPU — they exercise linkage and the
//! shared-memory / pure-Rust paths only. Tests that need a real GPU or a peer
//! sender are in the examples and are marked `#[ignore]` where present.
#![cfg(windows)]

/// Linking the native archive works end to end and the version call returns the
/// expected string. This is the single most important "is the native build
/// wired up correctly" check (proves cc + MSVC + system libs + CRT all agree).
#[test]
fn sdk_version_matches_pin() {
    let v = spout2::sdk_version();
    assert!(
        v.starts_with("2.007."),
        "unexpected Spout SDK version: {v:?}"
    );
    assert_eq!(v, spout2::SPOUT_SDK_VERSION);
}

#[cfg(feature = "dx")]
mod dx {
    /// Creating a DX receiver and enumerating senders touches only shared memory
    /// — no D3D11 device is created — so it is safe on a headless CI machine.
    #[test]
    fn receiver_enumeration_is_gpu_free() {
        let rx = spout2::dx::Receiver::new(None).expect("create receiver");
        // No assertion on contents (depends on what's running); must not panic.
        let _list = rx.sender_list();
        let _active = rx.active_sender();
        let _count = rx.sender_count();
    }

    /// Buffer-size validation happens before any FFI call, so it runs without a GPU.
    #[test]
    fn receive_image_rejects_undersized_buffer() {
        let mut rx = spout2::dx::Receiver::new(None).expect("create receiver");
        let mut small = vec![0u8; 16];
        let err = rx
            .receive_image(&mut small, 64, 64, false, false)
            .expect_err("undersized buffer must be rejected");
        assert!(
            matches!(err, spout2::SpoutError::BufferSize { expected, got }
            if expected == 64 * 64 * 4 && got == 16)
        );
    }

    /// Buffer-size validation must not wrap on huge dimensions in release builds.
    #[test]
    fn receive_image_rejects_dimension_overflow() {
        let mut rx = spout2::dx::Receiver::new(None).expect("create receiver");
        let mut small = vec![0u8; 16];
        let err = rx
            .receive_image(&mut small, u32::MAX, u32::MAX, false, false)
            .expect_err("overflowing dimensions must be rejected");
        assert!(matches!(
            err,
            spout2::SpoutError::DimensionOverflow {
                width: u32::MAX,
                height: u32::MAX,
                ..
            }
        ));
    }

    /// A name with an interior NUL is rejected cleanly rather than panicking.
    #[test]
    fn receiver_rejects_interior_nul_name() {
        match spout2::dx::Receiver::new(Some("bad\0name")) {
            Err(e) => assert_eq!(e, spout2::SpoutError::InvalidName),
            Ok(_) => panic!("interior NUL must be rejected"),
        }
    }

    /// Raw byte names reject NUL bytes but do not require UTF-8.
    #[test]
    fn receiver_accepts_non_utf8_name_bytes() {
        let rx = spout2::dx::Receiver::with_name_bytes(Some(b"camera-\xFF"))
            .expect("non-UTF-8 bytes without NUL are valid Spout names");
        assert!(!rx.is_connected());

        match spout2::dx::Receiver::with_name_bytes(Some(b"bad\0name")) {
            Err(e) => assert_eq!(e, spout2::SpoutError::InvalidName),
            Ok(_) => panic!("interior NUL must be rejected"),
        }
    }
}

#[cfg(feature = "gl")]
mod gl {
    /// Enumerating senders needs only shared memory — no GL context or GPU.
    #[test]
    fn sender_names_is_gpu_free() {
        let _names = spout2::gl::sender_names();
        let _raw_names = spout2::gl::sender_names_bytes();
    }
}
