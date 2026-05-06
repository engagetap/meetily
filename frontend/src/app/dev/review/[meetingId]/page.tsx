"use client";

import { useEffect, useState, useRef } from "react";
import { useParams } from "next/navigation";
import { invoke } from "@tauri-apps/api/core";

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
  accepted: number; // 0 | 1
  created_at: number;
  updated_at: number;
};

function fmtMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s.toString().padStart(2, "0")}.${(ms % 1000)
    .toString()
    .padStart(3, "0")}`;
}

export default function ReviewPage() {
  const params = useParams<{ meetingId: string }>();
  const meetingId = decodeURIComponent(params?.meetingId as string);

  const [shots, setShots] = useState<Screenshot[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<Screenshot[]>("screenshots_list", {
        meetingId,
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
    try {
      const n = await invoke<number>("screenshots_generate", { meetingId });
      console.log(`generated ${n} candidates`);
      await load();
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  return (
    <div style={{ padding: 24, fontFamily: "system-ui", maxWidth: 1000 }}>
      <h1 style={{ marginBottom: 8 }}>Review screenshots</h1>
      <p style={{ color: "#666", fontSize: 13 }}>
        Meeting: <code>{meetingId}</code>
      </p>

      <div style={{ display: "flex", gap: 12, marginTop: 16 }}>
        <button onClick={generate} style={{ padding: "8px 16px" }}>
          Generate from bookmarks
        </button>
        <button onClick={load} style={{ padding: "8px 16px" }}>
          Refresh
        </button>
      </div>

      {error && (
        <pre
          style={{
            color: "crimson",
            marginTop: 16,
            whiteSpace: "pre-wrap",
            fontSize: 13,
          }}
        >
          {error}
        </pre>
      )}

      {loading ? (
        <p style={{ marginTop: 24 }}>Loading…</p>
      ) : shots.length === 0 ? (
        <p style={{ marginTop: 24, color: "#999" }}>
          No candidates. Drop some bookmarks during a recording, then click
          "Generate from bookmarks".
        </p>
      ) : (
        <div style={{ marginTop: 24, display: "grid", gap: 24 }}>
          {shots.map((s) => (
            <ShotEditor key={s.id} shot={s} onChanged={load} />
          ))}
        </div>
      )}
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
  const [ts, setTs] = useState<number>(shot.timestamp_ms);
  const [caption, setCaption] = useState<string>(shot.caption ?? "");
  const [crop, setCrop] = useState<{
    x: number;
    y: number;
    w: number;
    h: number;
  } | null>(
    shot.crop_x != null && shot.crop_y != null && shot.crop_w != null && shot.crop_h != null
      ? {
          x: shot.crop_x,
          y: shot.crop_y,
          w: shot.crop_w,
          h: shot.crop_h,
        }
      : null
  );
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const debounceRef = useRef<number | null>(null);

  async function refreshPreview() {
    setErr(null);
    try {
      const url = await invoke<string>("screenshots_preview_frame", {
        meetingId: shot.meeting_id,
        timestampMs: ts,
        cropX: crop?.x ?? null,
        cropY: crop?.y ?? null,
        cropW: crop?.w ?? null,
        cropH: crop?.h ?? null,
      });
      setPreview(url);
    } catch (e) {
      setErr(String(e));
      setPreview(null);
    }
  }

  // Debounced preview refresh on ts/crop change.
  useEffect(() => {
    if (debounceRef.current) window.clearTimeout(debounceRef.current);
    debounceRef.current = window.setTimeout(() => {
      refreshPreview();
    }, 120);
    return () => {
      if (debounceRef.current) window.clearTimeout(debounceRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ts, crop?.x, crop?.y, crop?.w, crop?.h]);

  async function save() {
    setBusy(true);
    setErr(null);
    try {
      await invoke("screenshots_update", {
        id: shot.id,
        timestampMs: ts,
        crop: crop,
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
      // Save edits first so the accept extracts the right frame.
      await invoke("screenshots_update", {
        id: shot.id,
        timestampMs: ts,
        crop: crop,
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

  return (
    <div
      style={{
        border: "1px solid #ddd",
        borderRadius: 6,
        padding: 16,
        background: shot.accepted ? "#f3fbf4" : "#fff",
      }}
    >
      <div style={{ display: "flex", gap: 16 }}>
        <div style={{ flex: "0 0 320px" }}>
          {preview ? (
            <img
              src={preview}
              alt="preview"
              style={{
                width: "100%",
                height: "auto",
                borderRadius: 4,
                background: "#000",
              }}
            />
          ) : (
            <div
              style={{
                width: "100%",
                aspectRatio: "16 / 9",
                background: "#eee",
                borderRadius: 4,
                display: "grid",
                placeItems: "center",
                color: "#999",
                fontSize: 12,
              }}
            >
              Loading preview…
            </div>
          )}
        </div>

        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 8 }}>
          <div style={{ fontSize: 12, color: "#666" }}>
            <code>{shot.id.slice(0, 8)}</code> · source:{" "}
            <code>{shot.source}</code>
            {shot.accepted ? " · ✓ accepted" : ""}
          </div>

          <label style={{ fontSize: 13 }}>
            Timestamp: <code>{fmtMs(ts)}</code>{" "}
            <span style={{ color: "#666" }}>(±5s)</span>
            <input
              type="range"
              min={Math.max(0, shot.timestamp_ms - 5000)}
              max={shot.timestamp_ms + 5000}
              step={50}
              value={ts}
              onChange={(e) => setTs(parseInt(e.target.value))}
              style={{ width: "100%" }}
            />
          </label>

          <label style={{ fontSize: 13 }}>
            Caption
            <input
              type="text"
              value={caption}
              onChange={(e) => setCaption(e.target.value)}
              style={{
                width: "100%",
                padding: 6,
                fontSize: 13,
                marginTop: 4,
              }}
            />
          </label>

          <fieldset style={{ fontSize: 12, padding: 8, marginTop: 4 }}>
            <legend>Crop (pixels in source video)</legend>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <input
                type="checkbox"
                checked={crop != null}
                onChange={(e) =>
                  setCrop(
                    e.target.checked ? { x: 0, y: 0, w: 800, h: 600 } : null
                  )
                }
              />
              <span>Crop enabled</span>
            </div>
            {crop && (
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "auto 1fr auto 1fr",
                  gap: 6,
                  marginTop: 6,
                }}
              >
                <label>X</label>
                <input
                  type="number"
                  value={crop.x}
                  onChange={(e) =>
                    setCrop({ ...crop, x: parseInt(e.target.value || "0") })
                  }
                />
                <label>Y</label>
                <input
                  type="number"
                  value={crop.y}
                  onChange={(e) =>
                    setCrop({ ...crop, y: parseInt(e.target.value || "0") })
                  }
                />
                <label>W</label>
                <input
                  type="number"
                  value={crop.w}
                  onChange={(e) =>
                    setCrop({ ...crop, w: parseInt(e.target.value || "1") })
                  }
                />
                <label>H</label>
                <input
                  type="number"
                  value={crop.h}
                  onChange={(e) =>
                    setCrop({ ...crop, h: parseInt(e.target.value || "1") })
                  }
                />
              </div>
            )}
          </fieldset>

          <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
            <button onClick={save} disabled={busy} style={{ padding: "6px 12px" }}>
              Save edits
            </button>
            <button
              onClick={accept}
              disabled={busy}
              style={{ padding: "6px 12px", background: "#dff5e1" }}
            >
              {shot.accepted ? "Re-accept" : "Accept"}
            </button>
            <button
              onClick={reject}
              disabled={busy}
              style={{ padding: "6px 12px", background: "#fdecec" }}
            >
              Reject
            </button>
          </div>

          {err && (
            <pre style={{ color: "crimson", whiteSpace: "pre-wrap", fontSize: 12 }}>
              {err}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}
