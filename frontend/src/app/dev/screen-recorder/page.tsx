"use client";

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Display = {
  id: number;
  name: string;
  width: number;
  height: number;
  is_primary: boolean;
};

type RecordingMeta = {
  file_path: string;
  width: number;
  height: number;
  fps: number;
  codec: string;
  display_id: number;
  duration_ms: number;
};

type BookmarkResult = {
  id: string;
  meeting_id: string;
  timestamp_ms: number;
};

type ApiConfig = {
  host: string;
  port: number;
  token: string;
  url: string;
};

export default function ScreenRecorderDevPage() {
  const [displays, setDisplays] = useState<Display[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [recording, setRecording] = useState(false);
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const [meta, setMeta] = useState<RecordingMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [meetingId] = useState<string>("dev-" + Date.now());
  const [bookmarks, setBookmarks] = useState<BookmarkResult[]>([]);
  const [apiConfig, setApiConfig] = useState<ApiConfig | null>(null);

  useEffect(() => {
    invoke<Display[]>("screen_list_displays")
      .then((d) => {
        setDisplays(d);
        const primary = d.find((x) => x.is_primary) ?? d[0];
        if (primary) setSelected(primary.id);
      })
      .catch((e) => setError(String(e)));
    invoke<boolean>("screen_is_recording").then(setRecording);
    invoke<ApiConfig>("local_api_get_config").then(setApiConfig).catch(() => {});
  }, []);

  async function start() {
    setError(null);
    setMeta(null);
    setBookmarks([]);
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

  async function dropBookmark(label?: string) {
    setError(null);
    try {
      const b = await invoke<BookmarkResult>("bookmark_now", {
        label,
        source: "ui",
      });
      setBookmarks((prev) => [...prev, b]);
    } catch (e) {
      setError(String(e));
    }
  }

  async function rotateToken() {
    try {
      const cfg = await invoke<ApiConfig>("local_api_regenerate_token");
      setApiConfig(cfg);
    } catch (e) {
      setError(String(e));
    }
  }

  function fmtMs(ms: number): string {
    const totalSec = Math.floor(ms / 1000);
    const m = Math.floor(totalSec / 60);
    const s = totalSec % 60;
    return `${m}:${s.toString().padStart(2, "0")}.${(ms % 1000)
      .toString()
      .padStart(3, "0")}`;
  }

  function curlExample(): string {
    if (!apiConfig) return "";
    return `curl -X POST -H "Authorization: Bearer ${apiConfig.token}" ${apiConfig.url}/bookmark`;
  }

  return (
    <div style={{ padding: 24, fontFamily: "system-ui", maxWidth: 760 }}>
      <h1 style={{ marginBottom: 8 }}>Screen Recorder Dev</h1>
      <p style={{ color: "#666", fontSize: 13 }}>
        Phase 1A/1B smoke surface — exercises <code>screen_*</code>,{" "}
        <code>bookmark_now</code>, and the local HTTP API. Not the production
        UX.
      </p>
      <p>
        Meeting ID: <code>{meetingId}</code>
      </p>

      <h2 style={{ fontSize: 16, marginTop: 24 }}>Displays</h2>
      {displays.length === 0 && (
        <p style={{ color: "#a00" }}>
          No displays. If you've never granted Screen Recording permission, do
          so in System Settings → Privacy &amp; Security → Screen &amp; System
          Audio Recording, then reload.
        </p>
      )}
      <ul style={{ listStyle: "none", padding: 0 }}>
        {displays.map((d) => (
          <li key={d.id} style={{ padding: "4px 0" }}>
            <label>
              <input
                type="radio"
                name="display"
                value={d.id}
                checked={selected === d.id}
                onChange={() => setSelected(d.id)}
                style={{ marginRight: 8 }}
              />
              {d.name} ({d.width}×{d.height}
              {d.is_primary ? ", primary" : ""})
            </label>
          </li>
        ))}
      </ul>

      <div style={{ marginTop: 16, display: "flex", gap: 12 }}>
        {!recording ? (
          <button
            onClick={start}
            disabled={selected == null}
            style={{ padding: "8px 16px" }}
          >
            Start recording
          </button>
        ) : (
          <button onClick={stop} style={{ padding: "8px 16px" }}>
            Stop recording
          </button>
        )}
        <button
          onClick={() => dropBookmark()}
          disabled={!recording}
          style={{ padding: "8px 16px" }}
        >
          Bookmark now (UI)
        </button>
      </div>

      {recordingId && (
        <p style={{ marginTop: 16 }}>
          Recording ID: <code>{recordingId}</code>{" "}
          <span style={{ color: "#666" }}>
            (⇧⌘B drops a bookmark from anywhere)
          </span>
        </p>
      )}

      {bookmarks.length > 0 && (
        <div style={{ marginTop: 16 }}>
          <h3 style={{ fontSize: 14 }}>Bookmarks ({bookmarks.length})</h3>
          <ul style={{ fontSize: 13, paddingLeft: 20 }}>
            {bookmarks.map((b) => (
              <li key={b.id}>
                <code>{fmtMs(b.timestamp_ms)}</code> — {b.id}
              </li>
            ))}
          </ul>
        </div>
      )}

      {meta && (
        <pre
          style={{
            background: "#f4f4f4",
            padding: 12,
            marginTop: 16,
            borderRadius: 4,
            fontSize: 13,
          }}
        >
{JSON.stringify(meta, null, 2)}
        </pre>
      )}

      <hr style={{ margin: "32px 0" }} />

      <h2 style={{ fontSize: 16 }}>Local HTTP API</h2>
      {apiConfig ? (
        <div style={{ fontSize: 13 }}>
          <p>
            URL: <code>{apiConfig.url}</code>
            <br />
            Token: <code>{apiConfig.token}</code>{" "}
            <button onClick={rotateToken} style={{ marginLeft: 8 }}>
              Regenerate
            </button>
          </p>
          <p style={{ color: "#666" }}>
            Bookmark from a script (Companion / curl / scripts):
          </p>
          <pre
            style={{
              background: "#f4f4f4",
              padding: 12,
              borderRadius: 4,
              overflow: "auto",
            }}
          >
            {curlExample()}
          </pre>
          <p style={{ color: "#666", fontSize: 12 }}>
            Endpoints: <code>POST /bookmark</code>, <code>GET /status</code>,{" "}
            <code>POST /record/start</code>, <code>POST /record/stop</code>.
            All require the bearer token above.
          </p>
        </div>
      ) : (
        <p style={{ color: "#999" }}>API config not yet available.</p>
      )}

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
    </div>
  );
}
