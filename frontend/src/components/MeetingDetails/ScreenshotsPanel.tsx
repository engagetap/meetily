"use client";

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "@/lib/tauriGuard";
import { CropEditor, Crop } from "./CropEditor";

type Screenshot = {
  id: string;
  meeting_id: string;
  timestamp_ms: number;
  crop_x: number | null;
  crop_y: number | null;
  crop_w: number | null;
  crop_h: number | null;
  image_path: string | null;
  caption: string | null;
  source: string;
  confidence: number | null;
  accepted: number;
};

type EnrichResult = {
  processed: number;
  updated: number;
  skipped: number;
  errors: string[];
};

function fmtMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s.toString().padStart(2, "0")}.${(ms % 1000)
    .toString()
    .padStart(3, "0")}`;
}

/**
 * The full screenshot review surface for a meeting. Lives directly on the
 * meeting-details page. Shows accepted + pending candidates side by side,
 * with per-row controls for scrubbing the timestamp ±5s, cropping,
 * captioning, accepting, or rejecting. Top-level buttons generate
 * candidates from bookmarks + frame-diff, enrich pending captions via
 * Claude vision, and refresh.
 *
 * Audio meetings and screen recordings are persisted with independent ids.
 * The panel takes the audio meeting's id + created_at and resolves the
 * matching screen recording by timestamp proximity (via the
 * `screenshots_resolve_recording_meeting_id` Tauri command). All
 * subsequent operations use the resolved screen meeting id.
 */
export function ScreenshotsPanel({
  meetingId,
  meetingCreatedAtMs,
}: {
  meetingId: string;
  /**
   * Wall-clock timestamp (ms) the audio meeting started, used to find a
   * matching screen recording. If not provided, the panel falls back to
   * looking up screenshots by the audio meeting's id directly — which
   * only works if the screen recording was started with the same id.
   */
  meetingCreatedAtMs?: number;
}) {
  const [resolvedMeetingId, setResolvedMeetingId] = useState<string | null>(null);
  const [shots, setShots] = useState<Screenshot[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  // Resolve which "meeting_id" the screen recording uses. Try direct match
  // first (audio id == screen id, e.g. when ids were unified at start),
  // then fall back to time-proximity if a created_at is provided.
  useEffect(() => {
    if (!isTauri()) {
      setError("Open this inside the Meetily app.");
      setLoading(false);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const direct = await invoke<Screenshot[]>("screenshots_list", { meetingId });
        if (cancelled) return;
        if (direct.length > 0) {
          setResolvedMeetingId(meetingId);
          setShots(direct);
          setLoading(false);
          return;
        }
        if (meetingCreatedAtMs != null) {
          const screenId = await invoke<string | null>(
            "screenshots_resolve_recording_meeting_id",
            { nearMs: meetingCreatedAtMs, toleranceMs: 5 * 60 * 1000 }
          );
          if (cancelled) return;
          if (screenId) {
            const list = await invoke<Screenshot[]>("screenshots_list", {
              meetingId: screenId,
            });
            if (cancelled) return;
            setResolvedMeetingId(screenId);
            setShots(list);
            setLoading(false);
            return;
          }
        }
        // Nothing matched yet — keep the audio meeting id so generate / etc
        // still work if the user records something now.
        setResolvedMeetingId(meetingId);
        setShots([]);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [meetingId, meetingCreatedAtMs]);

  const activeMeetingId = resolvedMeetingId ?? meetingId;

  async function load() {
    if (!isTauri()) {
      setError("Open this inside the Meetily app.");
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<Screenshot[]>("screenshots_list", {
        meetingId: activeMeetingId,
      });
      setShots(list);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function generate() {
    setError(null);
    setBusy(true);
    try {
      await invoke<number>("screenshots_generate", { meetingId: activeMeetingId });
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function enrich() {
    setError(null);
    setBusy(true);
    try {
      const result = await invoke<EnrichResult>("screenshots_enrich_with_vision", {
        meetingId: activeMeetingId,
      });
      if (result.errors.length > 0) setError(result.errors.join("\n"));
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function captionFromTranscript() {
    setError(null);
    setBusy(true);
    try {
      await invoke("screenshots_caption_from_transcript", {
        meetingId: activeMeetingId,
      });
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const accepted = shots.filter((s) => s.accepted === 1);
  const pending = shots.filter((s) => s.accepted === 0);

  return (
    <div className="border-t border-gray-200 bg-white">
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-100">
        <div className="flex items-baseline gap-3">
          <h3 className="text-sm font-medium text-gray-700">Screenshots</h3>
          <span className="text-xs text-gray-500">
            {accepted.length} accepted · {pending.length} pending
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={generate}
            disabled={busy || loading}
            className="text-xs px-3 py-1.5 border border-gray-300 rounded hover:border-gray-400 disabled:opacity-50"
          >
            Generate
          </button>
          <button
            onClick={enrich}
            disabled={busy || loading || pending.length === 0}
            className="text-xs px-3 py-1.5 border border-gray-300 rounded hover:border-gray-400 disabled:opacity-50"
            title="Use the configured Anthropic API key to caption pending candidates (vision + transcript context)"
          >
            Caption with Claude
          </button>
          <button
            onClick={captionFromTranscript}
            disabled={busy || loading || pending.length === 0}
            className="text-xs px-3 py-1.5 border border-gray-300 rounded hover:border-gray-400 disabled:opacity-50"
            title="Caption from the transcript text near each screenshot (local, no API key)"
          >
            Caption from transcript
          </button>
          <button
            onClick={load}
            disabled={busy}
            className="text-xs px-3 py-1.5 border border-gray-300 rounded hover:border-gray-400 disabled:opacity-50"
          >
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <pre className="m-4 text-xs text-red-600 whitespace-pre-wrap">{error}</pre>
      )}

      <div className="p-4 space-y-4 max-h-[480px] overflow-y-auto">
        {loading ? (
          <p className="text-sm text-gray-500">Loading…</p>
        ) : shots.length === 0 ? (
          <p className="text-sm text-gray-500">
            No candidates yet. Click <strong>Generate</strong> to seed from
            bookmarks and frame-diff scanning.
          </p>
        ) : (
          shots.map((s) => (
            <ShotEditor key={s.id} shot={s} onChanged={load} />
          ))
        )}
      </div>
    </div>
  );
}

function ShotEditor({
  shot,
  onChanged,
}: {
  shot: Screenshot;
  onChanged: () => void;
}) {
  const [ts, setTs] = useState(shot.timestamp_ms);
  const [caption, setCaption] = useState(shot.caption ?? "");
  const [crop, setCrop] = useState<Crop | null>(
    shot.crop_x != null && shot.crop_y != null && shot.crop_w != null && shot.crop_h != null
      ? { x: shot.crop_x, y: shot.crop_y, w: shot.crop_w, h: shot.crop_h }
      : null
  );
  const [preview, setPreview] = useState<string | null>(null);
  const [previewSize, setPreviewSize] = useState<{ w: number; h: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const debounceRef = useRef<number | null>(null);

  async function refreshPreview() {
    setErr(null);
    try {
      // Always preview the FULL frame so the user can draw a crop on it.
      // The crop overlay is rendered on top in `CropEditor`; we only apply
      // the crop server-side at accept time (extracts the cropped PNG).
      const url = await invoke<string>("screenshots_preview_frame", {
        meetingId: shot.meeting_id,
        timestampMs: ts,
        cropX: null,
        cropY: null,
        cropW: null,
        cropH: null,
      });
      setPreview(url);

      // Probe natural dimensions so the crop overlay maps mouse coords →
      // source pixels correctly.
      const img = new Image();
      img.onload = () => setPreviewSize({ w: img.naturalWidth, h: img.naturalHeight });
      img.src = url;
    } catch (e) {
      setPreview(null);
      setErr(String(e));
    }
  }

  // Only timestamp affects what we fetch — the crop is overlaid client-side.
  useEffect(() => {
    if (debounceRef.current) window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(() => {
      refreshPreview();
    }, 120);
    return () => {
      if (debounceRef.current) window.clearTimeout(debounceRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ts]);

  async function save() {
    setBusy(true);
    setErr(null);
    try {
      await invoke("screenshots_update", {
        id: shot.id,
        timestampMs: ts,
        crop,
        caption: caption || null,
      });
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function accept() {
    setBusy(true);
    setErr(null);
    try {
      await invoke("screenshots_update", {
        id: shot.id,
        timestampMs: ts,
        crop,
        caption: caption || null,
      });
      await invoke("screenshots_accept", { id: shot.id });
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function reject() {
    setBusy(true);
    setErr(null);
    try {
      await invoke("screenshots_reject", { id: shot.id });
      onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  const accepted = shot.accepted === 1;

  return (
    <div
      className={`border rounded-md p-3 flex flex-col gap-3 ${
        accepted ? "border-green-300 bg-green-50/40" : "border-gray-200 bg-white"
      }`}
    >
      <div className="text-[11px] text-gray-500 flex items-center gap-2">
        <code>{shot.id.slice(0, 8)}</code>
        <span>· source: <code>{shot.source}</code></span>
        {accepted && <span className="text-green-700">✓ accepted</span>}
        {shot.confidence != null && (
          <span>· conf: {shot.confidence.toFixed(2)}</span>
        )}
      </div>

      {/* Visual crop editor — drag on the image to draw a rectangle, ⌘+wheel to zoom. */}
      {preview && previewSize ? (
        <CropEditor
          dataUrl={preview}
          naturalWidth={previewSize.w}
          naturalHeight={previewSize.h}
          crop={crop}
          onChange={setCrop}
          onClear={() => setCrop(null)}
        />
      ) : (
        <div className="w-full aspect-video bg-gray-100 rounded grid place-items-center text-[11px] text-gray-400">
          Loading preview…
        </div>
      )}

      <label className="text-xs text-gray-700">
        Timestamp <code>{fmtMs(ts)}</code>{" "}
        <span className="text-gray-400">(±5s)</span>
        <input
          type="range"
          min={Math.max(0, shot.timestamp_ms - 5000)}
          max={shot.timestamp_ms + 5000}
          step={50}
          value={ts}
          onChange={(e) => setTs(parseInt(e.target.value))}
          className="w-full mt-1"
        />
      </label>

      <label className="text-xs text-gray-700">
        Caption
        <input
          type="text"
          value={caption}
          onChange={(e) => setCaption(e.target.value)}
          className="block w-full mt-1 px-2 py-1 text-sm border border-gray-300 rounded"
        />
      </label>

      <div className="flex gap-2 mt-1">
        <button
          onClick={save}
          disabled={busy}
          className="text-xs px-3 py-1 border border-gray-300 rounded hover:border-gray-400 disabled:opacity-50"
        >
          Save edits
        </button>
        <button
          onClick={accept}
          disabled={busy}
          className="text-xs px-3 py-1 border border-green-400 bg-green-50 text-green-800 rounded hover:bg-green-100 disabled:opacity-50"
        >
          {accepted ? "Re-accept" : "Accept"}
        </button>
        <button
          onClick={reject}
          disabled={busy}
          className="text-xs px-3 py-1 border border-red-300 bg-red-50 text-red-700 rounded hover:bg-red-100 disabled:opacity-50"
        >
          Reject
        </button>
      </div>

      {err && (
        <pre className="text-[11px] text-red-600 whitespace-pre-wrap mt-1">
          {err}
        </pre>
      )}
    </div>
  );
}
