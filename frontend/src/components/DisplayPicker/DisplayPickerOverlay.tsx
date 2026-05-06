"use client";

/**
 * Fullscreen-ish overlay shown at the start of every meeting (when enabled)
 * so the user can pick which display to record. Promise-driven via a
 * window-global resolver so non-React modules (e.g. lib/screenRecording.ts)
 * can await `pickDisplay()` and proceed once a choice is made.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Monitor, Star, X, RefreshCw } from "lucide-react";
import {
  setDefaultDisplayPref,
  setRecordScreenPref,
  getRecordScreenPref,
  getDefaultDisplayPref,
} from "@/lib/screenRecording";

type Display = {
  id: number;
  name: string;
  width: number;
  height: number;
  is_primary: boolean;
};

type Resolver = (displayId: number | null) => void;

declare global {
  interface Window {
    __meetilyDisplayPicker?: {
      pick: () => Promise<number | null>;
    };
  }
}

const KEY_ALWAYS_ASK = "meetily.alwaysAskDisplay";

export function DisplayPickerOverlay() {
  const [open, setOpen] = useState(false);
  const [displays, setDisplays] = useState<Display[]>([]);
  const [thumbs, setThumbs] = useState<Record<number, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [alwaysAsk, setAlwaysAsk] = useState(true);
  const resolverRef = useRef<Resolver | null>(null);

  /**
   * Fetches a thumbnail for each display in parallel. Failures are
   * silently dropped — the card falls back to the generic monitor icon.
   */
  async function loadThumbnails(list: Display[]) {
    const entries = await Promise.all(
      list.map(async (d): Promise<[number, string] | null> => {
        try {
          const url = await invoke<string>("screen_capture_thumbnail", {
            displayId: d.id,
            maxWidth: 480,
          });
          return [d.id, url];
        } catch (e) {
          console.warn(`Thumbnail for display ${d.id} failed:`, e);
          return null;
        }
      })
    );
    setThumbs(Object.fromEntries(entries.filter(Boolean) as [number, string][]));
  }

  const pick = useCallback(async (): Promise<number | null> => {
    // If user opted out and a default is set, skip the picker silently.
    if (typeof window !== "undefined") {
      const ask = localStorage.getItem(KEY_ALWAYS_ASK);
      const savedDefault = getDefaultDisplayPref();
      if (ask === "false" && savedDefault) {
        const id = parseInt(savedDefault, 10);
        return Number.isFinite(id) ? id : null;
      }
    }
    const list = await invoke<Display[]>("screen_list_displays").catch(() => [] as Display[]);
    if (list.length === 0) return null;
    if (list.length === 1) return list[0].id; // No choice to make.

    setDisplays(list);
    setThumbs({});
    setOpen(true);
    setError(null);
    setAlwaysAsk(localStorage.getItem(KEY_ALWAYS_ASK) !== "false");
    void loadThumbnails(list);
    return new Promise((resolve) => {
      resolverRef.current = resolve;
    });
  }, []);

  // Expose to non-React callers.
  useEffect(() => {
    window.__meetilyDisplayPicker = { pick };
    return () => {
      if (window.__meetilyDisplayPicker?.pick === pick) {
        delete window.__meetilyDisplayPicker;
      }
    };
  }, [pick]);

  // Re-snap thumbnails every 2s while the picker is open so the previews
  // actually look live. Stops when the picker closes — no background load.
  useEffect(() => {
    if (!open || displays.length === 0) return;
    const id = window.setInterval(() => {
      void loadThumbnails(displays);
    }, 2000);
    return () => window.clearInterval(id);
  }, [open, displays]);

  function choose(id: number) {
    resolverRef.current?.(id);
    resolverRef.current = null;
    setOpen(false);
  }

  function cancel() {
    resolverRef.current?.(null);
    resolverRef.current = null;
    setOpen(false);
  }

  function disableScreenRecording() {
    setRecordScreenPref(false);
    cancel();
  }

  async function refresh() {
    setError(null);
    setThumbs({});
    try {
      const list = await invoke<Display[]>("screen_list_displays");
      setDisplays(list);
      if (list.length === 0) {
        setError(
          "No displays returned. Check Screen Recording permission in System Settings."
        );
      } else {
        void loadThumbnails(list);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  function toggleAlwaysAsk(next: boolean) {
    setAlwaysAsk(next);
    if (typeof window !== "undefined") {
      localStorage.setItem(KEY_ALWAYS_ASK, next ? "true" : "false");
    }
  }

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[1000] bg-black/80 backdrop-blur-sm flex items-center justify-center p-6">
      <div className="bg-white rounded-xl shadow-2xl max-w-3xl w-full max-h-[90vh] overflow-hidden flex flex-col">
        <div className="flex items-center justify-between px-6 py-4 border-b">
          <div>
            <h2 className="text-lg font-semibold text-gray-900">
              Pick a display to record
            </h2>
            <p className="text-sm text-gray-500 mt-0.5">
              {getRecordScreenPref()
                ? "This recording will capture audio + the display you choose."
                : "Screen recording is currently OFF — turn it on to record a display."}
            </p>
          </div>
          <button
            onClick={refresh}
            className="text-xs text-gray-500 hover:text-gray-700 flex items-center gap-1 px-2 py-1 border border-gray-200 rounded"
          >
            <RefreshCw className="w-3 h-3" /> Refresh
          </button>
        </div>

        {error && (
          <pre className="mx-6 mt-4 text-xs text-red-600 whitespace-pre-wrap">
            {error}
          </pre>
        )}

        <div className="flex-1 overflow-y-auto p-6">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            {displays.map((d) => {
              const aspect = d.height === 0 ? 16 / 9 : d.width / d.height;
              return (
                <button
                  key={d.id}
                  onClick={() => choose(d.id)}
                  className="text-left border border-gray-200 hover:border-blue-400 hover:shadow-md rounded-lg overflow-hidden transition-all bg-white"
                >
                  {thumbs[d.id] ? (
                    // eslint-disable-next-line @next/next/no-img-element
                    <img
                      src={thumbs[d.id]}
                      alt={`${d.name} preview`}
                      className="block w-full bg-black"
                      style={{ aspectRatio: `${aspect}`, objectFit: "contain" }}
                      draggable={false}
                    />
                  ) : (
                    <div
                      className="bg-gradient-to-br from-gray-700 to-gray-900 grid place-items-center text-gray-300"
                      style={{ aspectRatio: `${aspect}`, minHeight: 120 }}
                    >
                      <Monitor className="w-12 h-12 opacity-40" />
                    </div>
                  )}
                  <div className="p-3 flex items-center justify-between">
                    <div>
                      <div className="font-medium text-sm text-gray-900 flex items-center gap-1.5">
                        {d.name}
                        {d.is_primary && (
                          <span
                            className="inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded bg-amber-100 text-amber-800"
                            title="macOS primary display"
                          >
                            <Star className="w-2.5 h-2.5" /> Primary
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-gray-500 mt-0.5">
                        {d.width} × {d.height}
                      </div>
                    </div>
                    <span className="text-xs text-blue-600 font-medium">Choose →</span>
                  </div>
                </button>
              );
            })}
          </div>

          {displays.length === 0 && !error && (
            <p className="text-sm text-gray-500 text-center py-8">Loading displays…</p>
          )}
        </div>

        <div className="px-6 py-3 border-t bg-gray-50 flex items-center justify-between gap-4 flex-wrap">
          <label className="text-xs text-gray-700 flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={alwaysAsk}
              onChange={(e) => toggleAlwaysAsk(e.target.checked)}
              className="w-4 h-4"
            />
            Ask before every recording
            <span className="text-gray-400">
              (uncheck to default to your saved display)
            </span>
          </label>
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                if (displays.length > 0) {
                  setDefaultDisplayPref(String(displays[0].id));
                  choose(displays[0].id);
                }
              }}
              className="text-xs text-gray-600 hover:text-gray-900"
              title="Save current selection as default and choose"
            >
              Set as default
            </button>
            <button
              onClick={disableScreenRecording}
              className="text-xs px-3 py-1.5 border border-gray-300 rounded text-gray-700 hover:bg-white"
            >
              Skip — audio only
            </button>
            <button
              onClick={cancel}
              className="text-xs px-3 py-1.5 border border-gray-300 rounded text-gray-700 hover:bg-white flex items-center gap-1"
            >
              <X className="w-3 h-3" /> Cancel
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
