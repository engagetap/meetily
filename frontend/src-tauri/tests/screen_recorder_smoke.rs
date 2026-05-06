// End-to-end smoke test for the macOS ScreenCaptureKit-based recorder.
// Marked `#[ignore]` because it needs Screen Recording permission and a
// real display; run with:
//
//     cargo test --test screen_recorder_smoke -- --ignored --nocapture
//
// Skipped automatically on non-macOS targets.

#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn records_2_seconds_to_a_file() {
    use std::thread::sleep;
    use std::time::Duration;

    use app_lib::screen_recorder::ScreenRecorder;

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        let displays = app_lib::screen_recorder::recorder::list_displays()
            .expect("list displays");
        assert!(
            !displays.is_empty(),
            "at least one display + Screen Recording permission required"
        );
        let primary = displays.iter().find(|d| d.is_primary).unwrap_or(&displays[0]);

        let tmp = std::env::temp_dir().join("meetily_screen_recorder_smoke.mp4");
        let _ = std::fs::remove_file(&tmp);

        let recorder = ScreenRecorder::new();
        recorder
            .start(primary.id, &tmp, 30, 3000, false)
            .expect("recorder.start");
        sleep(Duration::from_secs(2));
        let meta = recorder.stop().expect("recorder.stop");

        assert!(
            meta.duration_ms >= 1500,
            "duration too short: {} ms",
            meta.duration_ms
        );
        assert_eq!(meta.fps, 30);

        let size = std::fs::metadata(&tmp).expect("output mp4 exists").len();
        assert!(size > 100_000, "recording too small: {} bytes", size);

        eprintln!(
            "smoke OK: {} bytes, {} ms, display {} {}x{}",
            size, meta.duration_ms, meta.display_id, meta.width, meta.height
        );
    });
}
