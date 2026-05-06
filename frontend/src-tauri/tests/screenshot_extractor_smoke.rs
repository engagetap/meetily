//! End-to-end-ish smoke test for the frame extractor.
//!
//! Records a 2-second mp4 (same flow as the Phase 1A smoke), then extracts
//! a frame at 1 second and verifies the bytes look like a PNG.
//!
//! Marked `#[ignore]` because it needs Screen Recording permission and a
//! real display:
//!     cargo test --test screenshot_extractor_smoke -- --ignored --nocapture

#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn extracts_a_png_frame_from_a_real_recording() {
    use std::thread::sleep;
    use std::time::Duration;

    use app_lib::screen_recorder::ScreenRecorder;
    use app_lib::screenshots::extract_frame;

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        let displays = app_lib::screen_recorder::recorder::list_displays().expect("list");
        assert!(!displays.is_empty(), "needs ≥1 display + screen permission");
        let primary = displays.iter().find(|d| d.is_primary).unwrap_or(&displays[0]);

        let tmp = std::env::temp_dir().join("meetily_screenshot_extractor_smoke.mp4");
        let _ = std::fs::remove_file(&tmp);

        let recorder = ScreenRecorder::new();
        recorder.start(primary.id, &tmp, 30, 3000).expect("start");
        sleep(Duration::from_secs(2));
        recorder.stop().expect("stop");
        // Tiny grace period for the file to be fully flushed.
        sleep(Duration::from_millis(200));

        let bytes = extract_frame(&tmp, 1000, None).expect("extract_frame");
        assert!(bytes.len() > 1000, "PNG too small: {} bytes", bytes.len());
        // PNG magic: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(
            &bytes[..8],
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
            "expected PNG magic header"
        );
        eprintln!("extractor smoke OK: {} bytes PNG", bytes.len());
    });
}
