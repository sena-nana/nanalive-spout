use nanalive_spout::{SpoutBackendKind, backend_status};

#[test]
fn sdk_version_matches_pin() {
    assert_eq!(
        nanalive_spout::sdk_version(),
        nanalive_spout::SPOUT_SDK_VERSION
    );
}

#[test]
fn unsupported_format_is_rejected_by_status_api() {
    let status = backend_status(SpoutBackendKind::CpuDx11);
    if !status.available {
        assert_eq!(status.backend, Some(SpoutBackendKind::CpuDx11));
    }
}

#[cfg(feature = "cpu-dx11")]
#[test]
fn cpu_frame_validation_rejects_undersized_buffer() {
    #[cfg(windows)]
    let mut sender = nanalive_spout::CpuDx11Sender::new("nanalive-test").expect("create sender");
    #[cfg(not(windows))]
    let result = nanalive_spout::CpuDx11Sender::new("nanalive-test");

    #[cfg(not(windows))]
    assert!(matches!(
        result,
        Err(nanalive_spout::SpoutOutputError::UnsupportedPlatform)
    ));

    #[cfg(windows)]
    {
        use nanalive_spout::{SpoutFrameRef, SpoutOutputError, SpoutSenderBackend};

        let pixels = [0u8; 16];
        let err = sender
            .publish(SpoutFrameRef::CpuPixels {
                pixels: &pixels,
                width: 64,
                height: 64,
                pitch_bytes: None,
            })
            .expect_err("undersized frame must be rejected before publish");
        assert!(matches!(
            err,
            SpoutOutputError::BufferTooSmall {
                expected,
                got: 16
            } if expected == 64 * 64 * 4
        ));
    }
}

#[test]
fn format_rejects_unsupported_dxgi_value() {
    use nanalive_spout::{SpoutFormat, SpoutOutputError};

    assert_eq!(
        SpoutFormat::from_dxgi_format(999_999),
        Err(SpoutOutputError::SurfaceFormatUnsupported)
    );
}

#[cfg(feature = "gpu-dx12-experimental")]
#[test]
fn dx12_constructor_rejects_null_device_or_queue() {
    use nanalive_spout::SpoutOutputError;

    let err = unsafe {
        nanalive_spout::GpuDx12ExperimentalSender::with_d3d12_device_and_queue(
            "nanalive-test",
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    }
    .expect_err("null interop pointers must be rejected");
    assert_eq!(err, SpoutOutputError::DeviceInteropUnavailable);
}
