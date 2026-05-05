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
        const primary = d.find((x) => x.is_primary) ?? d[0];
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
    <div style={{ padding: 24, fontFamily: "system-ui", maxWidth: 720 }}>
      <h1 style={{ marginBottom: 8 }}>Screen Recorder Dev</h1>
      <p style={{ color: "#666", fontSize: 13 }}>
        Phase 1A smoke screen — exercises the new <code>screen_*</code> Tauri
        commands. Not the production UX.
      </p>
      <p>
        Meeting ID: <code>{meetingId}</code>
      </p>

      <h2 style={{ fontSize: 16, marginTop: 24 }}>Displays</h2>
      {displays.length === 0 && (
        <p style={{ color: "#a00" }}>
          No displays returned. If you've never granted Screen Recording
          permission, do so in System Settings &rarr; Privacy &amp; Security
          &rarr; Screen &amp; System Audio Recording, then reload.
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

      <div style={{ marginTop: 16 }}>
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
      </div>

      {recordingId && (
        <p style={{ marginTop: 16 }}>
          Recording ID: <code>{recordingId}</code>
        </p>
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
