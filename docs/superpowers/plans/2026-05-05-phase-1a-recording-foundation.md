# Phase 1A — Recording Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a working ScreenCaptureKit-based screen + system-audio recorder to Meetily, exposed through Tauri commands, plus the database schema that subsequent phases will populate.

**Architecture:** A small Swift module (`ScreenRecorder.swift`) wraps `SCStream` and writes an `.mp4` via `AVAssetWriter`. It's compiled into a static lib by the `swift-rs` build helper and called from Rust through a thin `screen_recorder` module. Three new SQLite tables (`meeting_recordings`, `meeting_bookmarks`, `meeting_screenshots`) are created up front so all later phases can write to a stable schema. New Tauri commands let the frontend list displays, start a recording for a meeting, and stop it.

**Tech Stack:** Tauri 2 (Rust), Swift 5 + ScreenCaptureKit + AVFoundation, `swift-rs` crate, sqlx + SQLite, existing meetily code patterns.

---

## File structure

**New files:**
- `frontend/src-tauri/swift/ScreenRecorder.swift` — Swift implementation
- `frontend/src-tauri/swift/Package.swift` — Swift package manifest for `swift-rs`
- `frontend/src-tauri/src/screen_recorder/mod.rs` — Rust module entry
- `frontend/src-tauri/src/screen_recorder/bindings.rs` — `swift-rs` extern declarations
- `frontend/src-tauri/src/screen_recorder/recorder.rs` — `ScreenRecorder` Rust struct
- `frontend/src-tauri/src/screen_recorder/commands.rs` — Tauri commands
- `frontend/src-tauri/src/screen_recorder/types.rs` — shared types
- `frontend/src-tauri/migrations/20260505100000_add_video_recording_tables.sql` — schema
- `frontend/src-tauri/src/database/repositories/recording.rs` — `RecordingsRepository`
- `frontend/src-tauri/src/database/repositories/bookmark.rs` — `BookmarksRepository`
- `frontend/src-tauri/src/database/repositories/screenshot.rs` — `ScreenshotsRepository`
- `frontend/src-tauri/src/database/models.rs` — additions to existing file
- `frontend/src-tauri/tests/screen_recorder_smoke.rs` — integration smoke test

**Modified files:**
- `frontend/src-tauri/Cargo.toml` — add `swift-rs`
- `frontend/src-tauri/build.rs` — invoke `swift-rs` build for new module
- `frontend/src-tauri/src/lib.rs` — register module + commands
- `frontend/src-tauri/src/database/mod.rs` — re-export new repositories

**Module boundaries:**
- `screen_recorder` knows nothing about meetings, bookmarks, or screenshots — it is a self-contained "record a display to a file" component.
- `RecordingsRepository` is the only thing that ties a recording file path to a `meeting_id`.
- The new schema tables exist in this phase but are written to fully in later phases. `meeting_recordings` IS written in Phase 1A; `meeting_bookmarks` and `meeting_screenshots` get repository scaffolding only.

---

## Task 1: Add `swift-rs` dependency

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`

- [ ] **Step 1: Add `swift-rs` to macOS deps and build deps**

In `frontend/src-tauri/Cargo.toml`, find the `[target.'cfg(target_os = "macos")'.dependencies]` section and add:

```toml
swift-rs = "1.0.7"
```

Add or extend `[target.'cfg(target_os = "macos")'.build-dependencies]` (create the section if absent):

```toml
[target.'cfg(target_os = "macos")'.build-dependencies]
swift-rs = { version = "1.0.7", features = ["build"] }
```

- [ ] **Step 2: Verify Cargo accepts the manifest**

Run:
```bash
cd frontend/src-tauri && cargo metadata --no-deps --format-version 1 > /dev/null
```
Expected: exits 0, no errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/Cargo.toml
git commit -m "build(deps): add swift-rs for ScreenCaptureKit bindings"
```

---

## Task 2: Create Swift package skeleton

**Files:**
- Create: `frontend/src-tauri/swift/Package.swift`
- Create: `frontend/src-tauri/swift/Sources/ScreenRecorder/ScreenRecorder.swift` (placeholder)

- [ ] **Step 1: Write `Package.swift`**

```swift
// swift-tools-version:5.5
import PackageDescription

let package = Package(
    name: "ScreenRecorder",
    platforms: [.macOS(.v13)],
    products: [
        .library(name: "ScreenRecorder", type: .static, targets: ["ScreenRecorder"])
    ],
    dependencies: [
        .package(url: "https://github.com/Brendonovich/swift-rs", from: "1.0.7")
    ],
    targets: [
        .target(
            name: "ScreenRecorder",
            dependencies: [.product(name: "SwiftRs", package: "swift-rs")],
            path: "Sources/ScreenRecorder"
        )
    ]
)
```

- [ ] **Step 2: Write a placeholder Swift source so `swift build` works**

Create `frontend/src-tauri/swift/Sources/ScreenRecorder/ScreenRecorder.swift`:

```swift
import Foundation
import SwiftRs

@_cdecl("screen_recorder_ping")
public func ping() -> SRString {
    return SRString("pong")
}
```

- [ ] **Step 3: Verify Swift package builds standalone**

Run:
```bash
cd frontend/src-tauri/swift && swift build 2>&1 | tail -20
```
Expected: succeeds, prints `Build complete!` (or equivalent).

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/swift/
git commit -m "feat(screen_recorder): scaffold Swift package with ping"
```

---

## Task 3: Wire Swift build into `build.rs`

**Files:**
- Modify: `frontend/src-tauri/build.rs`

- [ ] **Step 1: Add `swift-rs` build call inside the macOS block**

Open `frontend/src-tauri/build.rs`. Inside the existing `#[cfg(target_os = "macos")]` block (just after the `cargo:rustc-link-lib` lines), add:

```rust
#[cfg(target_os = "macos")]
{
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=Cocoa");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-lib=framework=CoreVideo");

    swift_rs::SwiftLinker::new("13")
        .with_package("ScreenRecorder", "./swift")
        .link();
}
```

- [ ] **Step 2: Add the `use` import at top of `build.rs`**

At the very top (above existing `#[path = ...]` line), add:

```rust
#[cfg(target_os = "macos")]
use swift_rs as _; // re-export trick so the cfg-gated use compiles
```

Then inside the `#[cfg(target_os = "macos")]` block, prefix the `SwiftLinker::new(...)` call so it imports from the build dep:

```rust
        ::swift_rs::SwiftLinker::new("13")
            .with_package("ScreenRecorder", "./swift")
            .link();
```

(Using `::swift_rs` avoids ambiguity with any other crate.)

- [ ] **Step 3: Verify Rust crate compiles (will pull in Swift static lib)**

Run:
```bash
cd frontend/src-tauri && cargo build 2>&1 | tail -30
```
Expected: full compile succeeds. `screen_recorder_ping` symbol is now linked.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/build.rs
git commit -m "build(screen_recorder): link Swift package via swift-rs"
```

---

## Task 4: Add Rust `screen_recorder` module skeleton

**Files:**
- Create: `frontend/src-tauri/src/screen_recorder/mod.rs`
- Create: `frontend/src-tauri/src/screen_recorder/bindings.rs`
- Create: `frontend/src-tauri/src/screen_recorder/types.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Write `mod.rs`**

```rust
pub mod bindings;
pub mod commands;
pub mod recorder;
pub mod types;

pub use recorder::ScreenRecorder;
pub use types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
```

- [ ] **Step 2: Write `types.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMeta {
    pub file_path: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: String,
    pub display_id: u32,
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ScreenRecorderError {
    #[error("permission denied for screen recording")]
    PermissionDenied,
    #[error("display not found: {0}")]
    DisplayNotFound(u32),
    #[error("already recording")]
    AlreadyRecording,
    #[error("not currently recording")]
    NotRecording,
    #[error("io error: {0}")]
    Io(String),
    #[error("internal error: {0}")]
    Internal(String),
}
```

- [ ] **Step 3: Write `bindings.rs` with the `ping` extern (placeholder)**

```rust
#[cfg(target_os = "macos")]
use swift_rs::{swift, SRString};

#[cfg(target_os = "macos")]
swift!(pub fn screen_recorder_ping() -> SRString);
```

- [ ] **Step 4: Add an empty `commands.rs`**

```rust
// Tauri commands, populated in later tasks.
```

- [ ] **Step 5: Add a placeholder `recorder.rs`**

```rust
use crate::screen_recorder::types::ScreenRecorderError;

pub struct ScreenRecorder;

impl ScreenRecorder {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ScreenRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
pub fn _unused_error_ref(_e: &ScreenRecorderError) {}
```

- [ ] **Step 6: Register the module in `lib.rs`**

In `frontend/src-tauri/src/lib.rs`, find the existing `pub mod` declarations (audio, audio_v2, database, etc.) and add:

```rust
#[cfg(target_os = "macos")]
pub mod screen_recorder;
```

- [ ] **Step 7: Verify it compiles**

Run:
```bash
cd frontend/src-tauri && cargo check 2>&1 | tail -15
```
Expected: succeeds with at most warnings.

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/screen_recorder/ frontend/src-tauri/src/lib.rs
git commit -m "feat(screen_recorder): add Rust module skeleton"
```

---

## Task 5: Verify Rust↔Swift FFI works end-to-end (ping test)

**Files:**
- Create: `frontend/src-tauri/tests/screen_recorder_smoke.rs`

- [ ] **Step 1: Write the failing FFI smoke test**

```rust
#[cfg(target_os = "macos")]
#[test]
fn ffi_ping_returns_pong() {
    let result = unsafe { app_lib::screen_recorder::bindings::screen_recorder_ping() };
    assert_eq!(result.to_string(), "pong");
}
```

- [ ] **Step 2: Run it to see it fails compiling because `bindings` is private**

Run:
```bash
cd frontend/src-tauri && cargo test --test screen_recorder_smoke 2>&1 | tail -10
```
Expected: compile error or test failure.

- [ ] **Step 3: Make `bindings` reachable (re-export via `mod.rs`)**

Already done in Task 4 (`pub mod bindings;`). If still failing, ensure the `swift!` macro generates a `pub` function (it does when annotated `pub`).

- [ ] **Step 4: Run the test until it passes**

```bash
cd frontend/src-tauri && cargo test --test screen_recorder_smoke -- --nocapture 2>&1 | tail -15
```
Expected: PASS, output includes `pong`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/tests/screen_recorder_smoke.rs
git commit -m "test(screen_recorder): verify Swift FFI ping round-trip"
```

---

## Task 6: Implement `listDisplays` in Swift

**Files:**
- Modify: `frontend/src-tauri/swift/Sources/ScreenRecorder/ScreenRecorder.swift`

- [ ] **Step 1: Write `listDisplays` Swift function**

Replace the file contents with:

```swift
import Foundation
import ScreenCaptureKit
import SwiftRs

// MARK: - Display listing

@objc public class SRDisplay: NSObject {
    @objc public let id: UInt32
    @objc public let name: SRString
    @objc public let width: UInt32
    @objc public let height: UInt32
    @objc public let scale: Float
    @objc public let isPrimary: Bool

    public init(id: UInt32, name: String, width: UInt32, height: UInt32, scale: Float, isPrimary: Bool) {
        self.id = id
        self.name = SRString(name)
        self.width = width
        self.height = height
        self.scale = scale
        self.isPrimary = isPrimary
    }
}

@_cdecl("screen_recorder_list_displays")
public func listDisplays() -> SRObjectArray {
    let semaphore = DispatchSemaphore(value: 0)
    var collected: [SRDisplay] = []

    Task {
        defer { semaphore.signal() }
        do {
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            let primaryId = CGMainDisplayID()
            for d in content.displays {
                let cg = CGDisplayCreateImage(d.displayID)
                let scale = cg.map { Float(CGFloat($0.width) / CGFloat(d.width)) } ?? 1.0
                let display = SRDisplay(
                    id: UInt32(d.displayID),
                    name: "Display \(d.displayID)",
                    width: UInt32(d.width),
                    height: UInt32(d.height),
                    scale: scale,
                    isPrimary: d.displayID == primaryId
                )
                collected.append(display)
            }
        } catch {
            // Swallow — caller will see empty array if permission denied
        }
    }

    semaphore.wait()
    return SRObjectArray(collected)
}

// MARK: - Ping (kept for FFI smoke test)

@_cdecl("screen_recorder_ping")
public func ping() -> SRString {
    return SRString("pong")
}
```

- [ ] **Step 2: Build the Swift package**

```bash
cd frontend/src-tauri/swift && swift build 2>&1 | tail -10
```
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/swift/Sources/ScreenRecorder/ScreenRecorder.swift
git commit -m "feat(screen_recorder): list displays via ScreenCaptureKit"
```

---

## Task 7: Bind `listDisplays` in Rust

**Files:**
- Modify: `frontend/src-tauri/src/screen_recorder/bindings.rs`
- Create test in: `frontend/src-tauri/tests/screen_recorder_smoke.rs`

- [ ] **Step 1: Add the binding**

Append to `bindings.rs`:

```rust
#[cfg(target_os = "macos")]
use swift_rs::{SRObjectArray, SRObject, Bool};

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Debug)]
pub struct SrDisplay {
    pub id: u32,
    pub name: SRString,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub is_primary: Bool,
}

#[cfg(target_os = "macos")]
swift!(pub fn screen_recorder_list_displays() -> SRObjectArray<SrDisplay>);
```

- [ ] **Step 2: Add a Rust helper that converts the array to `Vec<DisplayInfo>`**

Append to `recorder.rs`:

```rust
use crate::screen_recorder::bindings;
use crate::screen_recorder::types::DisplayInfo;

#[cfg(target_os = "macos")]
pub fn list_displays() -> Vec<DisplayInfo> {
    let arr = unsafe { bindings::screen_recorder_list_displays() };
    arr.iter()
        .map(|d| DisplayInfo {
            id: d.id,
            name: d.name.to_string(),
            width: d.width,
            height: d.height,
            scale: d.scale,
            is_primary: d.is_primary.into(),
        })
        .collect()
}
```

- [ ] **Step 3: Add an integration test (will pass on machines with ≥1 display)**

Append to `tests/screen_recorder_smoke.rs`:

```rust
#[cfg(target_os = "macos")]
#[test]
fn list_displays_returns_at_least_one_when_permitted() {
    let displays = app_lib::screen_recorder::recorder::list_displays();
    // CI without screen permission may return empty — assert non-panicking instead
    for d in &displays {
        assert!(d.width > 0);
        assert!(d.height > 0);
    }
}
```

- [ ] **Step 4: Run it**

```bash
cd frontend/src-tauri && cargo test --test screen_recorder_smoke -- --nocapture 2>&1 | tail -15
```
Expected: PASS. On a dev machine with screen recording permission granted, prints at least one display.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/screen_recorder/bindings.rs frontend/src-tauri/src/screen_recorder/recorder.rs frontend/src-tauri/tests/screen_recorder_smoke.rs
git commit -m "feat(screen_recorder): expose list_displays from Rust"
```

---

## Task 8: Implement Swift `startRecording` / `stopRecording`

**Files:**
- Modify: `frontend/src-tauri/swift/Sources/ScreenRecorder/ScreenRecorder.swift`

- [ ] **Step 1: Add the recording engine class**

Append to the Swift file:

```swift
// MARK: - Recording engine

import AVFoundation
import CoreMedia

final class RecorderEngine: NSObject, SCStreamDelegate, SCStreamOutput {
    private var stream: SCStream?
    private var assetWriter: AVAssetWriter?
    private var videoInput: AVAssetWriterInput?
    private var audioInput: AVAssetWriterInput?
    private var sessionStartedAt: CMTime?
    private let queue = DispatchQueue(label: "io.meetily.screen_recorder")
    private(set) var outputPath: String = ""
    private(set) var width: UInt32 = 0
    private(set) var height: UInt32 = 0
    private(set) var fps: UInt32 = 30

    func start(displayID: CGDirectDisplayID, outputPath: String, fps: UInt32, bitrateKbps: UInt32) async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == displayID }) else {
            throw NSError(domain: "ScreenRecorder", code: 1, userInfo: [NSLocalizedDescriptionKey: "display not found"])
        }
        let filter = SCContentFilter(display: display, excludingWindows: [])

        let cfg = SCStreamConfiguration()
        cfg.width = display.width
        cfg.height = display.height
        cfg.minimumFrameInterval = CMTime(value: 1, timescale: Int32(fps))
        cfg.queueDepth = 6
        cfg.capturesAudio = true
        cfg.pixelFormat = kCVPixelFormatType_32BGRA
        cfg.showsCursor = true

        self.outputPath = outputPath
        self.width = UInt32(display.width)
        self.height = UInt32(display.height)
        self.fps = fps

        let url = URL(fileURLWithPath: outputPath)
        try? FileManager.default.removeItem(at: url)

        let writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        let videoSettings: [String: Any] = [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: display.width,
            AVVideoHeightKey: display.height,
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: Int(bitrateKbps) * 1000,
                AVVideoMaxKeyFrameIntervalKey: Int(fps) * 2,
                AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel
            ]
        ]
        let vIn = AVAssetWriterInput(mediaType: .video, outputSettings: videoSettings)
        vIn.expectsMediaDataInRealTime = true
        if writer.canAdd(vIn) { writer.add(vIn) }

        let audioSettings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 48000,
            AVNumberOfChannelsKey: 2,
            AVEncoderBitRateKey: 192_000
        ]
        let aIn = AVAssetWriterInput(mediaType: .audio, outputSettings: audioSettings)
        aIn.expectsMediaDataInRealTime = true
        if writer.canAdd(aIn) { writer.add(aIn) }

        self.assetWriter = writer
        self.videoInput = vIn
        self.audioInput = aIn

        let stream = SCStream(filter: filter, configuration: cfg, delegate: self)
        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: queue)
        self.stream = stream

        writer.startWriting()
        try await stream.startCapture()
    }

    func stop() async throws -> UInt64 {
        guard let stream = self.stream, let writer = self.assetWriter else {
            throw NSError(domain: "ScreenRecorder", code: 2, userInfo: [NSLocalizedDescriptionKey: "not recording"])
        }
        try await stream.stopCapture()
        videoInput?.markAsFinished()
        audioInput?.markAsFinished()

        let semaphore = DispatchSemaphore(value: 0)
        writer.finishWriting { semaphore.signal() }
        semaphore.wait()

        self.stream = nil
        self.assetWriter = nil
        self.videoInput = nil
        self.audioInput = nil
        self.sessionStartedAt = nil
        // Authoritative duration is computed in Rust (Instant::now().elapsed()).
        // Returning 0 here keeps the FFI surface intentionally minimal.
        return 0
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard CMSampleBufferDataIsReady(sampleBuffer), let writer = assetWriter else { return }

        if writer.status == .unknown {
            let pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
            writer.startSession(atSourceTime: pts)
            sessionStartedAt = pts
        }
        if writer.status == .failed { return }

        switch type {
        case .screen:
            if let input = videoInput, input.isReadyForMoreMediaData {
                input.append(sampleBuffer)
            }
        case .audio:
            if let input = audioInput, input.isReadyForMoreMediaData {
                input.append(sampleBuffer)
            }
        @unknown default: break
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        // Surface via writer.failed status next time a sample arrives; nothing to do here.
    }
}

// MARK: - C ABI

private let engine = RecorderEngine()

@_cdecl("screen_recorder_start")
public func startRecording(displayId: UInt32, outputPath: SRString, fps: UInt32, bitrateKbps: UInt32) -> Int32 {
    let path = outputPath.toString()
    let semaphore = DispatchSemaphore(value: 0)
    var rc: Int32 = 0
    Task {
        defer { semaphore.signal() }
        do {
            try await engine.start(displayID: CGDirectDisplayID(displayId), outputPath: path, fps: fps, bitrateKbps: bitrateKbps)
            rc = 0
        } catch {
            rc = -1
        }
    }
    semaphore.wait()
    return rc
}

@_cdecl("screen_recorder_stop")
public func stopRecording() -> UInt64 {
    let semaphore = DispatchSemaphore(value: 0)
    var duration: UInt64 = 0
    Task {
        defer { semaphore.signal() }
        do {
            duration = try await engine.stop()
        } catch {
            duration = 0
        }
    }
    semaphore.wait()
    return duration
}
```

- [ ] **Step 2: Build the Swift package**

```bash
cd frontend/src-tauri/swift && swift build 2>&1 | tail -15
```
Expected: succeeds. Warnings about unused values are fine; errors are not.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/swift/Sources/ScreenRecorder/ScreenRecorder.swift
git commit -m "feat(screen_recorder): implement start/stop via SCStream + AVAssetWriter"
```

---

## Task 9: Bind `start` / `stop` in Rust and write `ScreenRecorder` struct

**Files:**
- Modify: `frontend/src-tauri/src/screen_recorder/bindings.rs`
- Modify: `frontend/src-tauri/src/screen_recorder/recorder.rs`

- [ ] **Step 1: Add bindings**

Append to `bindings.rs`:

```rust
#[cfg(target_os = "macos")]
swift!(pub fn screen_recorder_start(
    display_id: u32,
    output_path: SRString,
    fps: u32,
    bitrate_kbps: u32
) -> i32);

#[cfg(target_os = "macos")]
swift!(pub fn screen_recorder_stop() -> u64);
```

- [ ] **Step 2: Replace placeholder `recorder.rs`**

Replace the contents of `recorder.rs` with:

```rust
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use crate::screen_recorder::bindings;
use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};

#[cfg(target_os = "macos")]
use swift_rs::SRString;

#[derive(Debug)]
struct ActiveSession {
    display_id: u32,
    output_path: PathBuf,
    fps: u32,
    started_at: Instant,
}

pub struct ScreenRecorder {
    active: Mutex<Option<ActiveSession>>,
}

impl ScreenRecorder {
    pub fn new() -> Self {
        Self { active: Mutex::new(None) }
    }

    pub fn list_displays(&self) -> Vec<DisplayInfo> {
        list_displays()
    }

    pub fn is_recording(&self) -> bool {
        self.active.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn start(
        &self,
        display_id: u32,
        output_path: &Path,
        fps: u32,
        bitrate_kbps: u32,
    ) -> Result<(), ScreenRecorderError> {
        let mut guard = self.active.lock().map_err(|_| ScreenRecorderError::Internal("mutex poisoned".into()))?;
        if guard.is_some() {
            return Err(ScreenRecorderError::AlreadyRecording);
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ScreenRecorderError::Io(e.to_string()))?;
        }

        let path_str = output_path
            .to_str()
            .ok_or_else(|| ScreenRecorderError::Io("non-UTF8 path".into()))?;

        #[cfg(target_os = "macos")]
        let rc = unsafe {
            bindings::screen_recorder_start(display_id, SRString::from(path_str), fps, bitrate_kbps)
        };
        #[cfg(not(target_os = "macos"))]
        let rc: i32 = -2;

        if rc != 0 {
            return Err(ScreenRecorderError::Internal(format!("swift returned {}", rc)));
        }

        *guard = Some(ActiveSession {
            display_id,
            output_path: output_path.to_path_buf(),
            fps,
            started_at: Instant::now(),
        });
        Ok(())
    }

    pub fn stop(&self) -> Result<RecordingMeta, ScreenRecorderError> {
        let mut guard = self.active.lock().map_err(|_| ScreenRecorderError::Internal("mutex poisoned".into()))?;
        let session = guard.take().ok_or(ScreenRecorderError::NotRecording)?;

        #[cfg(target_os = "macos")]
        let _swift_dur_ms = unsafe { bindings::screen_recorder_stop() };
        let duration_ms = session.started_at.elapsed().as_millis() as u64;

        Ok(RecordingMeta {
            file_path: session.output_path.to_string_lossy().to_string(),
            width: 0, // populated when probing the file in Phase 2; kept 0 here intentionally
            height: 0,
            fps: session.fps,
            codec: "h264".to_string(),
            display_id: session.display_id,
            duration_ms,
        })
    }
}

impl Default for ScreenRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
pub fn list_displays() -> Vec<DisplayInfo> {
    let arr = unsafe { bindings::screen_recorder_list_displays() };
    arr.iter()
        .map(|d| DisplayInfo {
            id: d.id,
            name: d.name.to_string(),
            width: d.width,
            height: d.height,
            scale: d.scale,
            is_primary: d.is_primary.into(),
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn list_displays() -> Vec<DisplayInfo> {
    Vec::new()
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cd frontend/src-tauri && cargo build 2>&1 | tail -20
```
Expected: succeeds.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/screen_recorder/bindings.rs frontend/src-tauri/src/screen_recorder/recorder.rs
git commit -m "feat(screen_recorder): Rust ScreenRecorder with start/stop guard"
```

---

## Task 10: Smoke-test recording end-to-end

**Files:**
- Modify: `frontend/src-tauri/tests/screen_recorder_smoke.rs`

- [ ] **Step 1: Write a (manually run, ignored by default) integration test**

Append:

```rust
#[cfg(target_os = "macos")]
#[test]
#[ignore] // requires Screen Recording permission; run with: cargo test --test screen_recorder_smoke -- --ignored
fn records_2_seconds_to_a_file() {
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration;
    use app_lib::screen_recorder::ScreenRecorder;

    let displays = app_lib::screen_recorder::recorder::list_displays();
    assert!(!displays.is_empty(), "needs at least one display + screen recording permission");
    let primary = displays.iter().find(|d| d.is_primary).unwrap_or(&displays[0]);

    let tmp = std::env::temp_dir().join("meetily_screen_recorder_smoke.mp4");
    let _ = std::fs::remove_file(&tmp);
    let recorder = ScreenRecorder::new();
    recorder.start(primary.id, &tmp, 30, 3000).expect("start");
    sleep(Duration::from_secs(2));
    let meta = recorder.stop().expect("stop");
    assert!(meta.duration_ms >= 1500, "duration too short: {}", meta.duration_ms);
    let path: PathBuf = meta.file_path.into();
    let size = std::fs::metadata(&path).expect("file exists").len();
    assert!(size > 100_000, "recording too small: {}", size);
}
```

- [ ] **Step 2: Run with `--ignored`** (manual, requires permission grant)

```bash
cd frontend/src-tauri && cargo test --test screen_recorder_smoke -- --ignored --nocapture 2>&1 | tail -20
```
Expected on a permitted machine: PASS, mp4 file ≥ ~100 KB written to `$TMPDIR/meetily_screen_recorder_smoke.mp4`.

If it fails for permission reasons, grant Screen Recording in System Settings → Privacy & Security and re-run.

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/tests/screen_recorder_smoke.rs
git commit -m "test(screen_recorder): end-to-end record-2s smoke (ignored)"
```

---

## Task 11: Database migration for new tables

**Files:**
- Create: `frontend/src-tauri/migrations/20260505100000_add_video_recording_tables.sql`

- [ ] **Step 1: Write migration SQL**

```sql
-- Recordings: one row per actual recording artifact on disk.
CREATE TABLE IF NOT EXISTS meeting_recordings (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    file_path     TEXT NOT NULL,
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER,
    width         INTEGER,
    height        INTEGER,
    fps           INTEGER,
    codec         TEXT,
    display_id    INTEGER,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_recordings_meeting ON meeting_recordings(meeting_id);

-- Bookmarks: live-marked timestamps. Phase 1B writes these.
CREATE TABLE IF NOT EXISTS meeting_bookmarks (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    timestamp_ms  INTEGER NOT NULL,
    label         TEXT,
    source        TEXT NOT NULL CHECK (source IN ('hotkey','api','ui')),
    created_at    INTEGER NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_bookmarks_meeting_ts ON meeting_bookmarks(meeting_id, timestamp_ms);

-- Screenshots: candidate + accepted screenshots. Phase 2 writes these.
CREATE TABLE IF NOT EXISTS meeting_screenshots (
    id            TEXT PRIMARY KEY,
    meeting_id    TEXT NOT NULL,
    timestamp_ms  INTEGER NOT NULL,
    crop_x        INTEGER,
    crop_y        INTEGER,
    crop_w        INTEGER,
    crop_h        INTEGER,
    image_path    TEXT,
    caption       TEXT,
    source        TEXT NOT NULL
                  CHECK (source IN ('bookmark','transcript_cue','frame_diff','cloud_vision')),
    confidence    REAL,
    accepted      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_screenshots_meeting_ts ON meeting_screenshots(meeting_id, timestamp_ms);
```

- [ ] **Step 2: Inspect existing migration loader to confirm filename pattern**

```bash
ls frontend/src-tauri/migrations/ | tail -5
grep -rn "migrations" frontend/src-tauri/src/database/ | head -10
```
Expected: pattern `YYYYMMDDhhmmss_name.sql`. Filename in this task matches.

- [ ] **Step 3: Build to ensure sqlx picks up the new migration on next app start (compile-time check only)**

```bash
cd frontend/src-tauri && cargo check 2>&1 | tail -10
```
Expected: succeeds.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/migrations/20260505100000_add_video_recording_tables.sql
git commit -m "feat(db): migration for recordings, bookmarks, screenshots"
```

---

## Task 12: Database models

**Files:**
- Modify: `frontend/src-tauri/src/database/models.rs`

- [ ] **Step 1: Append new models**

Append to `models.rs` (do not remove existing types):

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MeetingRecording {
    pub id: String,
    pub meeting_id: String,
    pub file_path: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<i64>,
    pub codec: Option<String>,
    pub display_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MeetingBookmark {
    pub id: String,
    pub meeting_id: String,
    pub timestamp_ms: i64,
    pub label: Option<String>,
    pub source: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MeetingScreenshot {
    pub id: String,
    pub meeting_id: String,
    pub timestamp_ms: i64,
    pub crop_x: Option<i64>,
    pub crop_y: Option<i64>,
    pub crop_w: Option<i64>,
    pub crop_h: Option<i64>,
    pub image_path: Option<String>,
    pub caption: Option<String>,
    pub source: String,
    pub confidence: Option<f64>,
    pub accepted: i64,
    pub created_at: i64,
    pub updated_at: i64,
}
```

(If `serde` / `sqlx::FromRow` imports already exist at the top of the file, skip the duplicates.)

- [ ] **Step 2: Verify it compiles**

```bash
cd frontend/src-tauri && cargo check 2>&1 | tail -10
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src-tauri/src/database/models.rs
git commit -m "feat(db): models for recordings, bookmarks, screenshots"
```

---

## Task 13: `RecordingsRepository`

**Files:**
- Create: `frontend/src-tauri/src/database/repositories/recording.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`
- Create test in: `frontend/src-tauri/src/database/repositories/recording.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the repository**

```rust
use crate::database::models::MeetingRecording;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct RecordingsRepository;

impl RecordingsRepository {
    pub async fn create(
        pool: &SqlitePool,
        meeting_id: &str,
        file_path: &str,
        display_id: Option<i64>,
    ) -> Result<MeetingRecording, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let started_at = Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO meeting_recordings (id, meeting_id, file_path, started_at, display_id) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(meeting_id)
        .bind(file_path)
        .bind(started_at)
        .bind(display_id)
        .execute(pool)
        .await?;

        Ok(MeetingRecording {
            id,
            meeting_id: meeting_id.to_string(),
            file_path: file_path.to_string(),
            started_at,
            ended_at: None,
            width: None,
            height: None,
            fps: None,
            codec: None,
            display_id,
        })
    }

    pub async fn finalize(
        pool: &SqlitePool,
        id: &str,
        ended_at: i64,
        width: Option<i64>,
        height: Option<i64>,
        fps: Option<i64>,
        codec: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE meeting_recordings SET ended_at=?, width=?, height=?, fps=?, codec=? WHERE id=?"
        )
        .bind(ended_at)
        .bind(width)
        .bind(height)
        .bind(fps)
        .bind(codec)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingRecording>, sqlx::Error> {
        sqlx::query_as::<_, MeetingRecording>(
            "SELECT * FROM meeting_recordings WHERE meeting_id = ? ORDER BY started_at ASC"
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_with_schema() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query(include_str!("../../../migrations/20260505100000_add_video_recording_tables.sql"))
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn create_then_finalize_roundtrip() {
        let pool = pool_with_schema().await;
        let r = RecordingsRepository::create(&pool, "m1", "/tmp/x.mp4", Some(1)).await.unwrap();
        assert_eq!(r.meeting_id, "m1");
        assert!(r.ended_at.is_none());
        RecordingsRepository::finalize(&pool, &r.id, 12345, Some(1920), Some(1080), Some(30), Some("h264")).await.unwrap();
        let list = RecordingsRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].ended_at, Some(12345));
        assert_eq!(list[0].width, Some(1920));
    }
}
```

- [ ] **Step 2: Re-export from `repositories/mod.rs`**

In `frontend/src-tauri/src/database/repositories/mod.rs`, add:

```rust
pub mod recording;
pub use recording::RecordingsRepository;
```

- [ ] **Step 3: Run the test**

```bash
cd frontend/src-tauri && cargo test --lib database::repositories::recording 2>&1 | tail -10
```
Expected: 1 test passes.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/database/repositories/recording.rs frontend/src-tauri/src/database/repositories/mod.rs
git commit -m "feat(db): RecordingsRepository with create/finalize/list"
```

---

## Task 14: `BookmarksRepository` (scaffold)

**Files:**
- Create: `frontend/src-tauri/src/database/repositories/bookmark.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`

- [ ] **Step 1: Write the repository**

```rust
use crate::database::models::MeetingBookmark;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct BookmarksRepository;

impl BookmarksRepository {
    pub async fn create(
        pool: &SqlitePool,
        meeting_id: &str,
        timestamp_ms: i64,
        label: Option<&str>,
        source: &str,
    ) -> Result<MeetingBookmark, sqlx::Error> {
        if !matches!(source, "hotkey" | "api" | "ui") {
            return Err(sqlx::Error::Protocol(format!("invalid source: {}", source)));
        }
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO meeting_bookmarks (id, meeting_id, timestamp_ms, label, source, created_at) VALUES (?,?,?,?,?,?)"
        )
        .bind(&id).bind(meeting_id).bind(timestamp_ms).bind(label).bind(source).bind(created_at)
        .execute(pool).await?;
        Ok(MeetingBookmark {
            id,
            meeting_id: meeting_id.to_string(),
            timestamp_ms,
            label: label.map(str::to_string),
            source: source.to_string(),
            created_at,
        })
    }

    pub async fn list_for_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingBookmark>, sqlx::Error> {
        sqlx::query_as::<_, MeetingBookmark>(
            "SELECT * FROM meeting_bookmarks WHERE meeting_id=? ORDER BY timestamp_ms ASC"
        ).bind(meeting_id).fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)").execute(&pool).await.unwrap();
        sqlx::query(include_str!("../../../migrations/20260505100000_add_video_recording_tables.sql")).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')").execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn rejects_invalid_source() {
        let pool = pool().await;
        let err = BookmarksRepository::create(&pool, "m1", 100, None, "bogus").await.unwrap_err();
        match err {
            sqlx::Error::Protocol(_) => {},
            other => panic!("expected Protocol, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn create_and_list_in_order() {
        let pool = pool().await;
        BookmarksRepository::create(&pool, "m1", 2000, Some("b"), "ui").await.unwrap();
        BookmarksRepository::create(&pool, "m1", 1000, Some("a"), "hotkey").await.unwrap();
        let list = BookmarksRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(list.iter().map(|b| b.timestamp_ms).collect::<Vec<_>>(), vec![1000, 2000]);
    }
}
```

- [ ] **Step 2: Re-export**

In `repositories/mod.rs` add:

```rust
pub mod bookmark;
pub use bookmark::BookmarksRepository;
```

- [ ] **Step 3: Run tests**

```bash
cd frontend/src-tauri && cargo test --lib database::repositories::bookmark 2>&1 | tail -10
```
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/database/repositories/bookmark.rs frontend/src-tauri/src/database/repositories/mod.rs
git commit -m "feat(db): BookmarksRepository scaffold + tests"
```

---

## Task 15: `ScreenshotsRepository` (scaffold)

**Files:**
- Create: `frontend/src-tauri/src/database/repositories/screenshot.rs`
- Modify: `frontend/src-tauri/src/database/repositories/mod.rs`

- [ ] **Step 1: Write the repository**

```rust
use crate::database::models::MeetingScreenshot;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct ScreenshotsRepository;

#[derive(Debug, Clone, Default)]
pub struct ScreenshotInput<'a> {
    pub meeting_id: &'a str,
    pub timestamp_ms: i64,
    pub crop: Option<(i64, i64, i64, i64)>, // x, y, w, h
    pub caption: Option<&'a str>,
    pub source: &'a str,
    pub confidence: Option<f64>,
    pub accepted: bool,
}

impl ScreenshotsRepository {
    pub async fn insert(pool: &SqlitePool, input: ScreenshotInput<'_>) -> Result<MeetingScreenshot, sqlx::Error> {
        if !matches!(input.source, "bookmark" | "transcript_cue" | "frame_diff" | "cloud_vision") {
            return Err(sqlx::Error::Protocol(format!("invalid source: {}", input.source)));
        }
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();
        let (cx, cy, cw, ch) = input.crop.map(|c| (Some(c.0), Some(c.1), Some(c.2), Some(c.3))).unwrap_or((None, None, None, None));
        sqlx::query(
            "INSERT INTO meeting_screenshots
                (id, meeting_id, timestamp_ms, crop_x, crop_y, crop_w, crop_h,
                 image_path, caption, source, confidence, accepted, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id).bind(input.meeting_id).bind(input.timestamp_ms)
        .bind(cx).bind(cy).bind(cw).bind(ch)
        .bind(input.caption).bind(input.source).bind(input.confidence)
        .bind(if input.accepted { 1 } else { 0 })
        .bind(now).bind(now)
        .execute(pool).await?;
        Ok(MeetingScreenshot {
            id,
            meeting_id: input.meeting_id.to_string(),
            timestamp_ms: input.timestamp_ms,
            crop_x: cx, crop_y: cy, crop_w: cw, crop_h: ch,
            image_path: None,
            caption: input.caption.map(str::to_string),
            source: input.source.to_string(),
            confidence: input.confidence,
            accepted: if input.accepted { 1 } else { 0 },
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn list_for_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingScreenshot>, sqlx::Error> {
        sqlx::query_as::<_, MeetingScreenshot>(
            "SELECT * FROM meeting_screenshots WHERE meeting_id=? ORDER BY timestamp_ms ASC"
        ).bind(meeting_id).fetch_all(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)").execute(&pool).await.unwrap();
        sqlx::query(include_str!("../../../migrations/20260505100000_add_video_recording_tables.sql")).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1','t','t','t')").execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn inserts_with_crop_and_lists() {
        let pool = pool().await;
        let input = ScreenshotInput {
            meeting_id: "m1",
            timestamp_ms: 5000,
            crop: Some((10, 20, 800, 600)),
            caption: Some("dashboard"),
            source: "bookmark",
            confidence: Some(1.0),
            accepted: true,
        };
        let row = ScreenshotsRepository::insert(&pool, input).await.unwrap();
        assert_eq!(row.crop_w, Some(800));
        let list = ScreenshotsRepository::list_for_meeting(&pool, "m1").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].accepted, 1);
    }

    #[tokio::test]
    async fn rejects_invalid_source() {
        let pool = pool().await;
        let input = ScreenshotInput { meeting_id: "m1", timestamp_ms: 0, source: "weird", ..Default::default() };
        let err = ScreenshotsRepository::insert(&pool, input).await.unwrap_err();
        match err {
            sqlx::Error::Protocol(_) => {},
            other => panic!("got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Re-export**

In `repositories/mod.rs`:

```rust
pub mod screenshot;
pub use screenshot::{ScreenshotsRepository, ScreenshotInput};
```

- [ ] **Step 3: Run tests**

```bash
cd frontend/src-tauri && cargo test --lib database::repositories::screenshot 2>&1 | tail -10
```
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/database/repositories/screenshot.rs frontend/src-tauri/src/database/repositories/mod.rs
git commit -m "feat(db): ScreenshotsRepository scaffold + tests"
```

---

## Task 16: `ScreenRecorder` Tauri commands

**Files:**
- Modify: `frontend/src-tauri/src/screen_recorder/commands.rs`
- Modify: `frontend/src-tauri/src/lib.rs`

- [ ] **Step 1: Inspect the existing AppState wiring**

The project shares the SQLite pool through an `AppState { db_manager: DatabaseManager }` struct that is `app.manage(...)`-d after the database is initialized. Commands access the pool via `state.db_manager.pool()`. See `frontend/src-tauri/src/database/commands.rs` for canonical examples.

- [ ] **Step 2: Write the commands**

Replace `commands.rs` contents with the macOS implementation, plus a non-macOS stub (kept in the same file behind `cfg`):

```rust
#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;
    use std::sync::Arc;

    use tauri::State;

    use crate::database::repositories::RecordingsRepository;
    use crate::state::AppState;
    use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
    use crate::screen_recorder::ScreenRecorder;

    pub struct ScreenRecorderState {
        pub recorder: Arc<ScreenRecorder>,
        pub current_recording_id: tokio::sync::Mutex<Option<String>>,
    }

    impl ScreenRecorderState {
        pub fn new() -> Self {
            Self {
                recorder: Arc::new(ScreenRecorder::new()),
                current_recording_id: tokio::sync::Mutex::new(None),
            }
        }
    }

    fn recordings_dir() -> Result<PathBuf, ScreenRecorderError> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| ScreenRecorderError::Io("no data dir".into()))?;
        Ok(base.join("Meetily").join("recordings"))
    }

    #[tauri::command]
    pub async fn screen_list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
        Ok(crate::screen_recorder::recorder::list_displays())
    }

    #[tauri::command]
    pub async fn screen_is_recording(
        state: State<'_, ScreenRecorderState>,
    ) -> Result<bool, ScreenRecorderError> {
        Ok(state.recorder.is_recording())
    }

    #[tauri::command]
    pub async fn screen_start_recording(
        meeting_id: String,
        display_id: u32,
        fps: Option<u32>,
        bitrate_kbps: Option<u32>,
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<String, ScreenRecorderError> {
        let dir = recordings_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| ScreenRecorderError::Io(e.to_string()))?;
        let filename = format!("{}-{}.mp4", meeting_id, chrono::Utc::now().timestamp_millis());
        let path = dir.join(filename);

        let pool = app_state.db_manager.pool();
        let row = RecordingsRepository::create(pool, &meeting_id, &path.to_string_lossy(), Some(display_id as i64))
            .await
            .map_err(|e| ScreenRecorderError::Internal(format!("db: {}", e)))?;

        state
            .recorder
            .start(display_id, &path, fps.unwrap_or(30), bitrate_kbps.unwrap_or(3000))?;

        let mut cur = state.current_recording_id.lock().await;
        *cur = Some(row.id.clone());
        Ok(row.id)
    }

    #[tauri::command]
    pub async fn screen_stop_recording(
        state: State<'_, ScreenRecorderState>,
        app_state: State<'_, AppState>,
    ) -> Result<RecordingMeta, ScreenRecorderError> {
        let meta = state.recorder.stop()?;

        let mut cur = state.current_recording_id.lock().await;
        if let Some(id) = cur.take() {
            let pool = app_state.db_manager.pool();
            let _ = RecordingsRepository::finalize(
                pool,
                &id,
                chrono::Utc::now().timestamp_millis(),
                None,
                None,
                Some(meta.fps as i64),
                Some(&meta.codec),
            )
            .await;
        }

        Ok(meta)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use tauri::State;

    use crate::state::AppState;
    use crate::screen_recorder::types::{DisplayInfo, RecordingMeta, ScreenRecorderError};

    pub struct ScreenRecorderState;
    impl ScreenRecorderState {
        pub fn new() -> Self {
            Self
        }
    }

    #[tauri::command]
    pub async fn screen_list_displays() -> Result<Vec<DisplayInfo>, ScreenRecorderError> {
        Ok(vec![])
    }
    #[tauri::command]
    pub async fn screen_is_recording(_s: State<'_, ScreenRecorderState>) -> Result<bool, ScreenRecorderError> {
        Ok(false)
    }
    #[tauri::command]
    pub async fn screen_start_recording(
        _meeting_id: String,
        _display_id: u32,
        _fps: Option<u32>,
        _bitrate_kbps: Option<u32>,
        _s: State<'_, ScreenRecorderState>,
        _a: State<'_, AppState>,
    ) -> Result<String, ScreenRecorderError> {
        Err(ScreenRecorderError::Internal("not supported on this platform".into()))
    }
    #[tauri::command]
    pub async fn screen_stop_recording(
        _s: State<'_, ScreenRecorderState>,
        _a: State<'_, AppState>,
    ) -> Result<RecordingMeta, ScreenRecorderError> {
        Err(ScreenRecorderError::Internal("not supported on this platform".into()))
    }
}

pub use imp::*;
```

`AppState` lives in `crate::state` (see `frontend/src-tauri/src/state.rs` and the existing `app.manage(AppState { db_manager })` in `database/setup.rs:33`).

- [ ] **Step 2: Add `dirs` dependency if missing**

```bash
cd frontend/src-tauri && grep '^dirs' Cargo.toml || cargo add dirs@5
```
Expected: dependency present after this.

- [ ] **Step 3: Make `screen_recorder` always present so `commands` compiles on every OS**

Change the `lib.rs` declaration introduced in Task 4 from:

```rust
#[cfg(target_os = "macos")]
pub mod screen_recorder;
```

to:

```rust
pub mod screen_recorder;
```

In `screen_recorder/mod.rs`, gate the recorder-specific submodules:

```rust
#[cfg(target_os = "macos")]
pub mod bindings;
#[cfg(target_os = "macos")]
pub mod recorder;
pub mod commands;
pub mod types;

#[cfg(target_os = "macos")]
pub use recorder::ScreenRecorder;
pub use types::{DisplayInfo, RecordingMeta, ScreenRecorderError};
```

- [ ] **Step 4: Register state in `lib.rs`**

Open `frontend/src-tauri/src/lib.rs` and find the `tauri::Builder::default()` fluent chain (around line 393). Add a new `.manage(...)` call alongside the existing ones (e.g. right after `.manage(audio::init_system_audio_state())`):

```rust
        .manage(screen_recorder::commands::ScreenRecorderState::new())
```

- [ ] **Step 5: Add the four commands to `tauri::generate_handler![...]`**

Append these four lines inside the existing `tauri::generate_handler![ ... ]` macro (anywhere in the list — convention is to put them near related sections; placing them at the end is fine):

```rust
            screen_recorder::commands::screen_list_displays,
            screen_recorder::commands::screen_is_recording,
            screen_recorder::commands::screen_start_recording,
            screen_recorder::commands::screen_stop_recording,
```

- [ ] **Step 4: Verify it compiles**

```bash
cd frontend/src-tauri && cargo build 2>&1 | tail -20
```
Expected: succeeds.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/screen_recorder/ frontend/src-tauri/src/lib.rs frontend/src-tauri/Cargo.toml frontend/src-tauri/Cargo.lock
git commit -m "feat(screen_recorder): Tauri commands wired into app state"
```

---

## Task 17: Frontend display picker + record/stop debug page

**Files:**
- Create: `frontend/src/app/dev/screen-recorder/page.tsx`

This page is a developer-facing smoke test screen — not part of the production UX, just a way to exercise the new commands end-to-end. Production UX comes in Phase 1B/2.

- [ ] **Step 1: Write the page**

```tsx
"use client";

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Display = { id: number; name: string; width: number; height: number; scale: number; isPrimary: boolean };
type RecordingMeta = { file_path: string; fps: number; codec: string; display_id: number; duration_ms: number };

export default function ScreenRecorderDevPage() {
  const [displays, setDisplays] = useState<Display[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [recording, setRecording] = useState(false);
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const [meta, setMeta] = useState<RecordingMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [meetingId] = useState<string>("dev-" + Date.now());

  useEffect(() => {
    invoke<Display[]>("screen_list_displays")
      .then((d) => {
        setDisplays(d);
        const primary = d.find((x) => x.isPrimary) ?? d[0];
        if (primary) setSelected(primary.id);
      })
      .catch((e) => setError(String(e)));
    invoke<boolean>("screen_is_recording").then(setRecording);
  }, []);

  async function start() {
    setError(null);
    setMeta(null);
    if (selected == null) return;
    try {
      const id = await invoke<string>("screen_start_recording", {
        meetingId,
        displayId: selected,
        fps: 30,
        bitrateKbps: 3000,
      });
      setRecordingId(id);
      setRecording(true);
    } catch (e) {
      setError(String(e));
    }
  }

  async function stop() {
    setError(null);
    try {
      const m = await invoke<RecordingMeta>("screen_stop_recording");
      setMeta(m);
      setRecording(false);
      setRecordingId(null);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div style={{ padding: 24, fontFamily: "system-ui" }}>
      <h1>Screen Recorder Dev</h1>
      <p>Meeting ID: <code>{meetingId}</code></p>
      <h2>Displays</h2>
      <ul>
        {displays.map((d) => (
          <li key={d.id}>
            <label>
              <input
                type="radio"
                name="display"
                value={d.id}
                checked={selected === d.id}
                onChange={() => setSelected(d.id)}
              />{" "}
              {d.name} ({d.width}×{d.height}{d.isPrimary ? ", primary" : ""})
            </label>
          </li>
        ))}
      </ul>
      <div style={{ marginTop: 16 }}>
        {!recording ? (
          <button onClick={start} disabled={selected == null}>Start recording</button>
        ) : (
          <button onClick={stop}>Stop recording</button>
        )}
      </div>
      {recordingId && <p>Recording ID: <code>{recordingId}</code></p>}
      {meta && (
        <pre style={{ background: "#f4f4f4", padding: 12, marginTop: 16 }}>
{JSON.stringify(meta, null, 2)}
        </pre>
      )}
      {error && <pre style={{ color: "crimson", marginTop: 16 }}>{error}</pre>}
    </div>
  );
}
```

- [ ] **Step 2: Run the app and exercise it**

```bash
cd frontend && pnpm install --frozen-lockfile 2>&1 | tail -3
cd frontend && ./dev-gpu.sh 2>&1 | tail -40 &
```

Wait for the app window to open, navigate to `http://localhost:1420/dev/screen-recorder` (or click through if accessible from the existing nav).

Manual verification:
- Display list populates
- Click "Start recording", wait 5 seconds, click "Stop"
- `meta.file_path` points to a real file under `~/Library/Application Support/Meetily/recordings/`
- The mp4 plays in QuickTime with both video and (system) audio.

If permission prompt appears, grant it and retry.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/app/dev/screen-recorder/page.tsx
git commit -m "feat(frontend): dev page to exercise screen recorder"
```

---

## Task 18: Final cleanup + smoke test for the lot

- [ ] **Step 1: Run the full test suite**

```bash
cd frontend/src-tauri && cargo test 2>&1 | tail -20
```
Expected: all `--lib` and non-ignored tests pass.

- [ ] **Step 2: Run lints**

```bash
cd frontend/src-tauri && cargo clippy --all-targets -- -D warnings 2>&1 | tail -20
```
Fix any new warnings introduced by this branch's code.

- [ ] **Step 3: Re-run the manual end-to-end test**

```bash
cd frontend/src-tauri && cargo test --test screen_recorder_smoke -- --ignored --nocapture 2>&1 | tail -15
```
Expected: 2-second recording produces a valid mp4.

- [ ] **Step 4: Commit any cleanup**

```bash
git add -u
git commit -m "chore: clippy + cleanup after Phase 1A"
```

---

## Phase 1A complete

The branch now has:

- A working ScreenCaptureKit recorder accessible from Rust.
- Three Tauri commands: `screen_list_displays`, `screen_start_recording`, `screen_stop_recording`, plus `screen_is_recording`.
- Three new tables (`meeting_recordings`, `meeting_bookmarks`, `meeting_screenshots`) and repositories for each.
- A dev page at `/dev/screen-recorder` to exercise the stack manually.

**Deferred to later phases** (do not address in Phase 1A):

- **Microphone audio in the mp4.** Phase 1A captures system audio only via SCStream. Per the spec, the recording must eventually carry the mixed mic+system stream Whisper consumes, or a sidecar `<id>.mic.m4a`. This is a Phase 1B concern and depends on how cleanly the existing audio pipeline can fork a copy.
- **Permission flow UX.** If Screen Recording permission is missing, `screen_list_displays` returns an empty array. A friendly prompt with a "Open System Settings" button is Phase 1B.
- **Auto-start recording when a meeting starts.** The dev page makes up its own `meeting_id`; integration with the real meeting lifecycle is Phase 1B.

**Next phases** (separate plans):

- **Phase 1B** — Bookmarks: global hotkey via `tauri-plugin-global-shortcut`, in-app HUD button, the local HTTP API server (`axum`) with bearer-token auth, settings panel for hotkey + API token, Companion docs, mic muxing or sidecar, permission UX.
- **Phase 2** — Screenshot pipeline: Python `ScreenshotPicker` (transcript cues + pHash frame-diff), Rust `FrameExtractor` via AVFoundation, Scribe-style review UI (scrub + crop + caption + accept/reject), embedding in transcript / summary / highlights gallery.
- **Phase 3** — Cloud vision opt-in (BYO key, mirroring existing summary-provider settings) and StreamDeck/Companion integration docs + sample export.

---

## Self-review notes (engineer reading this plan: skim these before starting)

- The Swift recorder uses `SCStream` + `AVAssetWriter`. The PTS-based session start (`writer.startSession(atSourceTime: pts)`) is essential — using wall-clock time produces mp4s that QuickTime refuses to play.
- The duration returned from `screen_recorder_stop` in Swift is approximate. The Rust side uses `Instant::now().elapsed()` as the authoritative duration to avoid relying on the Swift-side calculation that's intentionally simplified.
- `dirs` crate is used for the app data directory (`~/Library/Application Support/Meetily/recordings`). If the project already has its own helper for app paths, prefer that — search for `app_data_dir` or similar before adding a new dependency.
- The dev page deliberately bypasses the production `meetings` flow (it makes up a `meeting_id`). Real integration with the recording lifecycle (auto-start when a meeting starts) is Phase 1B work.
- The non-macOS stubs in `commands.rs` exist so the project still compiles on Windows/Linux. They return errors at runtime — the frontend is expected to feature-detect via `screen_list_displays` returning an empty array.
- If `swift-rs` build fails on a clean checkout, ensure Xcode command-line tools are installed: `xcode-select --install`.
