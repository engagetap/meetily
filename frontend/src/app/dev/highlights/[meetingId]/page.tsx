"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { invoke } from "@tauri-apps/api/core";

type Screenshot = {
  id: string;
  meeting_id: string;
  timestamp_ms: number;
  caption: string | null;
  accepted: number; // 0 | 1
  source: string;
};

function fmtMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export default function HighlightsPage() {
  const params = useParams<{ meetingId: string }>();
  const meetingId = decodeURIComponent(params?.meetingId as string);

  const [shots, setShots] = useState<Screenshot[]>([]);
  const [imgs, setImgs] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const list = await invoke<Screenshot[]>("screenshots_list", {
          meetingId,
        });
        const accepted = list.filter((s) => s.accepted === 1);
        setShots(accepted);
        const entries: [string, string][] = [];
        for (const s of accepted) {
          try {
            const url = await invoke<string>("screenshots_read_image", {
              id: s.id,
            });
            entries.push([s.id, url]);
          } catch {
            // skip — image may not be on disk yet
          }
        }
        setImgs(Object.fromEntries(entries));
      } catch (e) {
        setError(String(e));
      }
    })();
  }, [meetingId]);

  return (
    <div style={{ padding: 24, fontFamily: "system-ui", maxWidth: 1100 }}>
      <h1 style={{ marginBottom: 8 }}>Highlights</h1>
      <p style={{ color: "#666", fontSize: 13 }}>
        Meeting: <code>{meetingId}</code> ·{" "}
        <Link href={`/dev/review/${encodeURIComponent(meetingId)}`}>Review</Link>
      </p>

      {error && (
        <pre style={{ color: "crimson", whiteSpace: "pre-wrap", fontSize: 13 }}>
          {error}
        </pre>
      )}

      {shots.length === 0 ? (
        <p style={{ marginTop: 24, color: "#999" }}>
          No accepted screenshots yet. Accept some on the review page first.
        </p>
      ) : (
        <div
          style={{
            marginTop: 24,
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
            gap: 16,
          }}
        >
          {shots.map((s) => (
            <figure
              key={s.id}
              style={{
                margin: 0,
                background: "#fff",
                border: "1px solid #ddd",
                borderRadius: 6,
                overflow: "hidden",
              }}
            >
              {imgs[s.id] ? (
                <img
                  src={imgs[s.id]}
                  alt={s.caption ?? ""}
                  style={{ width: "100%", display: "block" }}
                />
              ) : (
                <div
                  style={{
                    aspectRatio: "16 / 9",
                    background: "#eee",
                    display: "grid",
                    placeItems: "center",
                    color: "#999",
                    fontSize: 12,
                  }}
                >
                  Loading…
                </div>
              )}
              <figcaption style={{ padding: 10, fontSize: 13 }}>
                <strong>{fmtMs(s.timestamp_ms)}</strong>{" "}
                <span style={{ color: "#666" }}>({s.source})</span>
                {s.caption && <div style={{ marginTop: 4 }}>{s.caption}</div>}
              </figcaption>
            </figure>
          ))}
        </div>
      )}
    </div>
  );
}
