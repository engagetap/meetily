"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { invoke } from "@tauri-apps/api/core";

type Screenshot = {
  id: string;
  meeting_id: string;
  timestamp_ms: number;
  caption: string | null;
  accepted: number;
  source: string;
};

function fmtMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/// Horizontal strip of accepted screenshots for a meeting. Renders below the
/// transcript+summary split. Each tile shows the timestamp, caption, and the
/// PNG thumbnail. Clicking a tile opens the review page focused on that
/// screenshot (Phase 3 plumbing).
export function HighlightsStrip({ meetingId }: { meetingId: string }) {
  const [shots, setShots] = useState<Screenshot[]>([]);
  const [imgs, setImgs] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const list = await invoke<Screenshot[]>("screenshots_list", {
          meetingId,
        });
        const accepted = list.filter((s) => s.accepted === 1);
        if (cancelled) return;
        setShots(accepted);

        const entries: [string, string][] = [];
        for (const s of accepted) {
          try {
            const url = await invoke<string>("screenshots_read_image", {
              id: s.id,
            });
            entries.push([s.id, url]);
          } catch {
            // missing image on disk; skip silently
          }
        }
        if (cancelled) return;
        setImgs(Object.fromEntries(entries));
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [meetingId]);

  if (error) {
    return (
      <div className="border-t border-gray-200 bg-white p-3 text-xs text-red-600">
        Highlights: {error}
      </div>
    );
  }

  if (shots.length === 0) {
    return (
      <div className="border-t border-gray-200 bg-white px-4 py-2 text-xs text-gray-500 flex items-center gap-3">
        <span>No screenshots yet.</span>
        <Link
          href={`/dev/review/${encodeURIComponent(meetingId)}`}
          className="underline"
        >
          Generate / review
        </Link>
      </div>
    );
  }

  return (
    <div className="border-t border-gray-200 bg-white">
      <div className="flex items-center justify-between px-4 pt-3">
        <h3 className="text-sm font-medium text-gray-700">
          Screenshots ({shots.length})
        </h3>
        <div className="text-xs text-gray-500">
          <Link
            href={`/dev/review/${encodeURIComponent(meetingId)}`}
            className="mr-3 underline"
          >
            Review
          </Link>
          <Link
            href={`/dev/highlights/${encodeURIComponent(meetingId)}`}
            className="underline"
          >
            Gallery
          </Link>
        </div>
      </div>
      <div className="overflow-x-auto px-4 py-3">
        <div className="flex gap-3" style={{ minWidth: "max-content" }}>
          {shots.map((s) => (
            <Link
              key={s.id}
              href={`/dev/review/${encodeURIComponent(meetingId)}#${s.id}`}
              className="block shrink-0 w-48 border border-gray-200 rounded overflow-hidden hover:border-gray-400"
            >
              {imgs[s.id] ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img
                  src={imgs[s.id]}
                  alt={s.caption ?? ""}
                  className="w-full block aspect-video object-cover bg-black"
                />
              ) : (
                <div className="w-full aspect-video bg-gray-100 grid place-items-center text-[11px] text-gray-400">
                  Loading…
                </div>
              )}
              <div className="px-2 py-1 text-xs">
                <div className="font-medium">{fmtMs(s.timestamp_ms)}</div>
                {s.caption && (
                  <div className="text-gray-600 line-clamp-2">{s.caption}</div>
                )}
              </div>
            </Link>
          ))}
        </div>
      </div>
    </div>
  );
}
