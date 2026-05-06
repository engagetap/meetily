# Video Recording + Contextual Screenshots — Design

**Status:** draft, brainstorm-approved
**Date:** 2026-05-05
**Scope:** macOS-only v1; upstream-friendly

## Problem

Meetily today captures audio and produces transcripts + summaries. There is no record of what was on screen during a meeting, so visual context — slides, dashboards, demos, code — is lost. Re-watching a meeting to extract a single moment is impractical, and users have no way to point at "this was the important frame" while a meeting is happening.

We want to:

1. Archive the meeting visually as a synchronized A/V file.
2. Let users mark important moments live with zero UI friction.
3. Surface a small, curated set of screenshots in the meeting notes — anchored to bookmarks, expanded by AI suggestions, refined by the user post-meeting.

## Goals

1. Capture a synchronized A/V archive of the user-chosen display for every meeting.
2. Allow live "must-include" bookmarks via global hotkey, in-app button, or a local HTTP API (for StreamDeck via Bitfocus Companion).
3. After the meeting, run a suggestion pipeline (bookmarks + transcript cues + frame-diff, with optional opt-in cloud vision) that produces candidate screenshots.
4. Provide a Scribe-style review UI that lets the user, for each candidate: nudge the source timestamp, drag-to-crop the still, edit the caption, accept or reject. Edits remain non-destructive.
5. Make accepted screenshots first-class citizens in the existing app: inline in the transcript, embedded in the AI summary, and listed in a highlights gallery.
6. Stay privacy-first by default. Cloud vision is strictly opt-in and bring-your-own-key.

## Non-goals (v1)

- Multi-display capture or window-only capture (single chosen display).
- Windows or Linux support.
- Live screenshot suggestions during the meeting.
- Editing the recording itself (no trimming, no overlays).
- A native Elgato Stream Deck plugin — local API only in v1; Companion users get full functionality, native plugin is a follow-up.

## High-level architecture

```
┌──────────────────────────┐    ┌────────────────────────┐
│  Frontend (Next.js)      │    │  Local Control API     │
│  - Recording controls    │◀──▶│  127.0.0.1:<port>      │
│  - Review/Scribe UI      │    │  POST /bookmark        │
│  - Highlights gallery    │    │  POST /record/start    │
│  - Settings              │    │  POST /record/stop     │
└────────────┬─────────────┘    │  GET  /status          │
             │ Tauri commands   │  Bearer token auth     │
             ▼                  └────────────┬───────────┘
┌──────────────────────────────────────────────────────────┐
│  Tauri / Rust core                                       │
│  - ScreenRecorder (Swift bridge → ScreenCaptureKit)      │
│  - Bookmark store                                        │
│  - Local API server (axum)                               │
│  - Hotkey listener (tauri-plugin-global-shortcut)        │
└────────────┬─────────────────────────────────────────────┘
             │
             ▼                ┌──────────────────────────────┐
┌──────────────────────────┐  │  Python backend              │
│  Recording artifacts     │  │  - existing transcription    │
│  ~/Library/Application   │  │  - new: ScreenshotPicker     │
│  Support/Meetily/        │  │      transcript cues         │
│  recordings/<id>.mp4     │  │      pHash frame-diff        │
│                          │  │      optional cloud vision   │
└──────────────────────────┘  └──────────────────────────────┘

A Rust-side FrameExtractor (AVFoundation, called from Tauri commands)
serves the review UI's live scrubbing — frontend → Tauri → Rust → PNG,
no Python round-trip on the hot path.
```

Components are designed for clean isolation:

- **ScreenRecorder** owns ScreenCaptureKit and emits an `.mp4`. Knows nothing about meetings or bookmarks.
- **BookmarkStore** owns timestamps. Knows nothing about video.
- **LocalApiServer** is a thin HTTP shim — every endpoint maps to one Tauri command.
- **ScreenshotPicker** (Python, offline) consumes `(transcript, video_path, bookmarks)` and emits screenshot candidates. Pure function over its inputs.
- **FrameExtractor** (Rust, on-demand) turns `(video_path, timestamp_ms, crop_rect?)` into PNG bytes. Pure. Used by the review UI's scrub slider via a Tauri command — kept in Rust to avoid Python round-trips on the hot path.
- **ReviewUI** is a stateless renderer over `meeting_screenshots` rows.

Each can be tested in isolation; replacing any one (e.g., swapping ScreenCaptureKit for AVFoundation later, or swapping pHash for a different heuristic) does not require touching the others.

## Component design

### 1. ScreenRecorder (Rust + Swift bridge)

ScreenCaptureKit is Swift/Obj-C only. We add a small Swift module compiled into the Tauri app and exposed to Rust through a C ABI.

- API:
  - `screen_recorder_start(out_path: &str, display_id: u32, fps: u32, bitrate_kbps: u32) -> Result<RecordingHandle>`
  - `screen_recorder_stop(handle: RecordingHandle) -> Result<RecordingMeta>`
  - `screen_recorder_list_displays() -> Vec<DisplayInfo { id, name, width, height, scale, is_primary }>`
- Captures system audio via ScreenCaptureKit and screen video in one stream. Microphone audio comes from Meetily's existing CoreAudio capture path and is muxed into the same `.mp4` alongside system audio (single mixed stereo track matching what Whisper already consumes). If single-track muxing turns out impractical given Meetily's current audio pipeline, the fallback is a sidecar `<id>.mic.m4a` plus the mp4 — but the mp4 must always be playable on its own.
- Output: H.264 video + AAC audio in `.mp4` at 30 fps, ~3 Mbps default (≈1.3 GB/hour at 1080p — acceptable local archive). Configurable.
- Permissions: macOS Screen Recording permission must be granted; the app surfaces a clear request flow on first use, with a "Open System Settings" button.
- Lifecycle: start is idempotent (returns existing handle if already recording), stop flushes the muxer cleanly. Fatal errors (disk full, permission revoked mid-recording) finalize the file at the last keyframe and surface a toast.

### 2. BookmarkStore

A bookmark is `(meeting_id, timestamp_ms, label?, source)` where `source ∈ {hotkey, api, ui}`. No frame snapshot at bookmark time — the video frame at that timestamp is the source of truth and can be nudged later. Keeps the live path zero-latency.

Triggers:

- Global hotkey (default `⇧⌘B`) via `tauri-plugin-global-shortcut`.
- In-app button on the recording HUD.
- `POST /bookmark` on the local API.

All three paths call the same Tauri command. The HUD shows a brief flash + counter when a bookmark lands so the user has feedback.

### 3. Local Control API

`axum` HTTP server bound to `127.0.0.1` on a random port at app start. Port + a generated bearer token are written to `~/Library/Application Support/Meetily/api.json` (mode 0600). Endpoints:

- `POST /bookmark` — body optional `{label?: string}` → drops a bookmark at "now"; returns `{id, timestamp_ms}`. 409 if not currently recording.
- `POST /record/start` — body optional `{display_id?: u32}` → starts recording; returns `{recording_id}`.
- `POST /record/stop` → stops; returns `{recording_id, duration_ms, file_path}`.
- `GET /status` → `{recording: bool, elapsed_ms?, bookmark_count?, recording_id?}` for Companion button feedback states.
- Auth: `Authorization: Bearer <token>`. 401 on mismatch.

Bitfocus Companion is configured via the Generic HTTP module against `http://127.0.0.1:<port>` with the bearer token; we ship a one-page guide and a sample Companion config export.

### 4. ScreenshotPicker (post-meeting pipeline)

Runs in the existing Python backend when recording stops, in the same job that already produces summaries. Takes:

- `transcript: List[Segment(text, start_ms, end_ms, speaker)]`
- `video_path`
- `bookmarks: List[(timestamp_ms, label?)]`
- `config: { use_cloud_vision: bool, max_screenshots: int (default 12) }`

Pipeline stages:

1. **Bookmark anchors.** Every bookmark becomes a candidate with `source=bookmark`, `confidence=1.0`. These are pre-accepted in the review UI.
2. **Transcript cues.** Regex + small heuristic over the transcript for phrases like *"look at this," "as you (can) see," "this slide/screen/diagram," "here's,"* topic shifts (cosine distance between adjacent windows of segments), and proper-noun introductions. Each cue produces a candidate with `source=transcript_cue` and `confidence` from heuristic strength.
3. **Frame-diff filter.** Decode video at 1 fps via PyAV (preferred over shelling out to `ffmpeg` — avoids a binary dependency since PyAV ships ffmpeg libs as wheels), compute perceptual hashes with `imagehash.phash`. For each candidate timestamp, snap to the nearest *stable, novel* frame within ±3 s — i.e. a frame whose pHash distance from its predecessor is small (stable) and from any previously-selected frame is large (novel). Drops near-duplicate candidates.
4. **Optional cloud vision (opt-in).** If the user has enabled and configured a vision provider (Anthropic / OpenAI / Gemini), each surviving candidate is sent the still + a context window of the transcript and asked to (a) score usefulness 0–1 and (b) propose a caption. Below-threshold candidates are dropped; provided captions become the default caption.
5. **Cap and rank.** Keep at most `max_screenshots`, prioritizing bookmarks (always kept), then by confidence.

Outputs `meeting_screenshots` rows with `accepted=False` (except bookmarks: `accepted=True`).

### 5. FrameExtractor (Rust)

Pure helper exposed via a Tauri command: `(video_path, timestamp_ms, crop_rect?) -> PNG bytes`. Implemented in Rust using AVFoundation's `AVAssetImageGenerator` (the macOS-native path: fast random seeks, no extra dependency). Used for live thumbnails as the user scrubs and for writing the final accepted PNG to disk on accept.

Rationale for living in Rust rather than Python: scrubbing is the hot path of the review UI; a Python round-trip per slider tick would be unacceptable. AVFoundation gives us native, low-latency seeks for free on macOS.

### 6. Review UI (Scribe-style)

After the existing summary step, a "Review screenshots" panel appears. For each candidate:

- Thumbnail rendered via FrameExtractor at the candidate timestamp + crop rect.
- **Frame nudge slider** (default ±5 s) — scrubbing updates the thumbnail in real time. Backend serves frames over a Tauri command; debounce 80 ms.
- **Crop tool** — drag corners on the still to define `crop_rect`. Toggle "reset to full frame."
- **Caption editor** — single-line text field, prefilled by AI when available.
- **Accept / Reject** buttons. Bookmarks default to accepted.
- "Edit later" affordance everywhere an accepted screenshot is shown re-opens this same editor; edits remain non-destructive because the source video, `timestamp_ms`, and `crop_rect` are retained.

### 7. Embedding into existing views

One canonical store (`meeting_screenshots`) feeds three views:

- **Transcript view.** Accepted screenshots inserted between transcript blocks at their `timestamp_ms`. Click → opens the editor. Hover → "open at moment in player."
- **AI summary.** The summary prompt is extended with `[screenshot:<id> @ <timestamp> caption="..."]` markers; the LLM is instructed to embed them at relevant points in the markdown summary. Renderer resolves markers to inline images.
- **Highlights gallery.** A new tab on the meeting page: ordered grid of accepted screenshots with captions. Each tile links to (a) the moment in the transcript and (b) `meeting.mp4#t=<seconds>` in a built-in `<video>` player.

## Data model additions

New SQLite tables (additive — no migrations to existing schema):

```sql
CREATE TABLE meeting_recordings (
  id            TEXT PRIMARY KEY,
  meeting_id    TEXT NOT NULL,
  file_path     TEXT NOT NULL,
  started_at    INTEGER NOT NULL,
  ended_at      INTEGER,
  width         INTEGER,
  height        INTEGER,
  fps           INTEGER,
  codec         TEXT,
  display_id    TEXT,
  FOREIGN KEY (meeting_id) REFERENCES meetings(id)
);

CREATE TABLE meeting_bookmarks (
  id            TEXT PRIMARY KEY,
  meeting_id    TEXT NOT NULL,
  timestamp_ms  INTEGER NOT NULL,
  label         TEXT,
  source        TEXT NOT NULL CHECK (source IN ('hotkey','api','ui')),
  created_at    INTEGER NOT NULL,
  FOREIGN KEY (meeting_id) REFERENCES meetings(id)
);

CREATE TABLE meeting_screenshots (
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
  FOREIGN KEY (meeting_id) REFERENCES meetings(id)
);
CREATE INDEX idx_screenshots_meeting_ts ON meeting_screenshots(meeting_id, timestamp_ms);
```

Crop fields are nullable — null = full frame. `image_path` is null until accepted.

## Settings (new panel)

- **Recording**
  - Enabled / disabled (default: enabled)
  - Default display (or "ask each time")
  - Bitrate (default 3000 kbps), fps (default 30)
- **Storage**
  - Recordings folder (default `~/Library/Application Support/Meetily/recordings`)
  - Retention: keep until manually deleted (default), with "warn at 50 GB used" reminder
- **Hotkeys**
  - Bookmark hotkey (default `⇧⌘B`)
  - Record-toggle hotkey (default `⇧⌘R`)
- **Local API**
  - Enable / disable
  - Port (default: random)
  - Regenerate token, copy `curl` example, copy Companion JSON
- **Cloud vision (opt-in)**
  - Provider (Anthropic / OpenAI / Gemini / none)
  - API key (reuses existing provider settings pattern)
  - Min confidence threshold to keep a suggestion

## Error & edge cases

- **Permission revoked mid-recording.** Finalize the mp4, surface a clear toast pointing to System Settings.
- **Disk full.** Stop recording cleanly at the last keyframe, retain whatever was written, notify.
- **Hotkey conflict.** On registration failure, fall back to in-app button + API and notify the user.
- **Meeting has no recording (audio-only or recording disabled).** Bookmarks and the review step are simply skipped; existing flow is unchanged.
- **Display unplugged mid-recording.** ScreenCaptureKit emits an error; we stop cleanly and prompt the user.
- **Very long meetings (>3 hours).** No special handling beyond capping `max_screenshots` and the existing per-file mp4 size; we do not auto-segment in v1.
- **Token leakage on the local API.** Token rotates on app start; re-binding to a random port limits exposure if a stale token leaks. CORS is locked to deny browsers; only same-machine non-browser callers (Companion) work.

## Testing strategy

- **Unit (Rust):** ScreenRecorder lifecycle (start idempotency, stop flushes, list_displays parses), BookmarkStore CRUD, API auth + endpoint contracts (with a mocked recorder).
- **Unit (Python):** ScreenshotPicker stages each tested independently with fixed inputs (golden transcripts, fixture mp4s).
- **Unit (Frontend):** Review UI state machine — accept/reject/nudge/crop transitions over a fake `meeting_screenshots` set.
- **Integration:** end-to-end record → bookmark via API → stop → pipeline runs → expected number of candidates produced from a known fixture meeting.
- **Manual checklist:** Scribe scrub UI feels smooth (<100 ms response), summary embedding renders, multi-display picker, hotkey conflict fallback, permission denial flow.

## Build sequence

1. **ScreenRecorder native + Rust binding + Tauri commands.** No UI yet; expose a debug menu to start/stop.
2. **Recording HUD + display picker + settings (recording section only).**
3. **BookmarkStore + global hotkey + in-app button + HUD feedback.**
4. **Local API server + auth + endpoint set + settings (local API section).**
5. **FrameExtractor + Python ScreenshotPicker (bookmarks + transcript cues + frame-diff only).**
6. **Review UI: scrub + crop + caption + accept/reject.**
7. **Embedding into transcript, summary prompt, highlights gallery tab.**
8. **Cloud vision opt-in path.**
9. **Companion integration docs + sample export.**

Each stage produces a runnable, testable slice; later stages do not require gutting earlier ones.

## Risks / open questions

- **Mic + system-audio muxing.** Meetily already mixes mic + system audio for transcription. Default plan is to mux that same mixed stream into the mp4. If the existing audio pipeline cannot easily fork a copy into the recorder, the fallback is a sidecar `<id>.mic.m4a`. Decided in the implementation phase based on what the existing audio code permits.
- **Frame extraction performance.** Live scrubbing relies on `AVAssetImageGenerator`, which is fast for keyframe seeks. Default H.264 keyframe interval (every 2 s) should keep scrub feedback under 100 ms; if not, we tighten the GOP at the cost of slightly higher bitrate, or pre-extract a low-res proxy on stop.
- **Upstream willingness.** Privacy-pure default + opt-in cloud is upstream-friendly, but the maintainers may have opinions on adding video to a project that has so far been audio-only. Worth raising as an issue/discussion before significant implementation effort.

## Out of scope follow-ups

- Native Elgato Stream Deck plugin.
- Windows port (DXGI / Windows.Graphics.Capture).
- Window-only capture mode.
- Auto-redaction of sensitive on-screen content.
- Trimming / editing the recording itself.
- ~~True transcript-inline embedding~~ — DONE. `VirtualizedTranscriptView`
  now accepts an optional `screenshots` array and merges it with
  `segments` by timestamp into a single timeline; both kinds are
  rendered (with virtualization above the threshold) using the same
  `useVirtualizer` instance. `TranscriptPanel` fetches accepted
  screenshots via `screenshots_list` + `screenshots_read_image` and
  passes them down.
- ~~LLM-side summary embedding~~ — DONE. The Rust summary processor
  injects a `<screenshots_available>` block into the user prompt with
  numbered markers and an instruction to weave `[screenshot:N]`
  references into the output. After the LLM returns, those markers are
  expanded into `![caption](sshot:<uuid>)` markdown images. The
  frontend `BlockNoteSummaryView` resolves `sshot:<uuid>` URIs to
  data URLs (via `screenshots_read_image`) before parsing the
  markdown into BlockNote blocks, so the database stays small while
  rendered summaries show the images inline.

## What's actually implemented (engagetap fork)

- **Phase 1A:** `screen_recorder` Rust module using cidre +
  SCRecordingOutput, four Tauri commands, `meeting_recordings` /
  `meeting_bookmarks` / `meeting_screenshots` tables and repositories.
  macOS 15+ floor accepted.
- **Phase 1B:** `bookmark_now` command, ⇧⌘B global hotkey via
  `tauri-plugin-global-shortcut`, axum-based local HTTP API
  (`/bookmark` / `/status` / `/record/start` / `/record/stop`) with
  bearer-token auth, persisted config at
  `~/Library/Application Support/Meetily/api.json`.
- **Phase 2:** `screenshots` Rust module — ffmpeg-based frame extractor,
  bookmarks-to-candidates picker, frame-diff scanner (sum-of-absolute
  differences over 32×18 luma thumbnails, configurable max + dedup),
  Scribe-style review UI (`/dev/review/[meetingId]`) with timestamp
  scrub + crop + caption + accept/reject, highlights gallery
  (`/dev/highlights/[meetingId]`), `HighlightsStrip` integrated into
  the existing meeting-details page.
- **Phase 3:** Cloud vision enrichment via Anthropic Claude (uses the
  existing claude API key from Settings; no-op if absent).
- **Mic capture:** opt-in via `capture_mic` flag on
  `screen_start_recording` (the local API mirrors it). When enabled,
  mic audio is muxed into the mp4 by SCRecordingOutput automatically.
