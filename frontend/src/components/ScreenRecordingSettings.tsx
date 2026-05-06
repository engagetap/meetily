"use client";

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Monitor, MonitorOff, Check, AlertTriangle, RefreshCw } from "lucide-react";
import {
  hasScreenRecordingPermission,
  openScreenRecordingSettings,
  setRecordScreenPref,
  setRecordMicPref,
  setDefaultDisplayPref,
} from "@/lib/screenRecording";
import { useScreenRecordingPrefs } from "@/hooks/useScreenRecordingPrefs";

type Display = {
  id: number;
  name: string;
  width: number;
  height: number;
  is_primary: boolean;
};

/**
 * Settings card for the screen-recording feature. Surfaces:
 *   - macOS Screen Recording permission status + a button to open System Settings
 *   - Toggle: include screen capture when starting a meeting (default ON)
 *   - Toggle: also mux microphone audio into the mp4 (default OFF; needs Mic permission)
 *   - Default display picker — primary by default; pick a specific monitor to record
 *   - Pointer to where the Anthropic API key (used for screenshot vision) is set
 *
 * Designed to drop into the existing Preferences modal — keeps the same
 * visual idiom (section header + label/control rows).
 */
export function ScreenRecordingSettings() {
  const [permitted, setPermitted] = useState<boolean | null>(null);
  const [displays, setDisplays] = useState<Display[]>([]);
  const { recordScreen, recordMic, defaultDisplayId } = useScreenRecordingPrefs();

  async function refreshPermissionAndDisplays() {
    const granted = await hasScreenRecordingPermission();
    setPermitted(granted);
    try {
      const list = await invoke<Display[]>("screen_list_displays");
      setDisplays(list);
    } catch {
      setDisplays([]);
    }
  }

  useEffect(() => {
    refreshPermissionAndDisplays();
  }, []);

  return (
    <section className="space-y-4">
      <div className="flex items-baseline justify-between">
        <h4 className="text-lg font-semibold text-gray-900">Screen Recording</h4>
        <button
          onClick={refreshPermissionAndDisplays}
          className="text-xs text-gray-500 hover:text-gray-700 flex items-center gap-1"
        >
          <RefreshCw className="w-3 h-3" /> Refresh
        </button>
      </div>

      {/* Permission status row */}
      <div
        className={`flex items-start gap-3 p-3 rounded-md border ${
          permitted
            ? "border-green-200 bg-green-50"
            : permitted === false
              ? "border-amber-200 bg-amber-50"
              : "border-gray-200 bg-gray-50"
        }`}
      >
        <div className="mt-0.5">
          {permitted ? (
            <Check className="w-5 h-5 text-green-600" />
          ) : (
            <AlertTriangle className="w-5 h-5 text-amber-600" />
          )}
        </div>
        <div className="flex-1 text-sm">
          {permitted == null ? (
            <p className="text-gray-600">Checking macOS Screen Recording permission…</p>
          ) : permitted ? (
            <p className="text-green-800">
              <strong>Screen Recording permission: granted.</strong> Meetily can capture
              your screen during meetings.
            </p>
          ) : (
            <>
              <p className="text-amber-800">
                <strong>Screen Recording permission not granted.</strong> macOS blocks
                screen capture until you grant it.
              </p>
              <button
                onClick={() => openScreenRecordingSettings()}
                className="mt-2 text-xs px-3 py-1 border border-amber-300 bg-white hover:bg-amber-100 rounded"
              >
                Open System Settings
              </button>
              <p className="text-xs text-amber-700 mt-2">
                After granting, you must fully quit and relaunch Meetily — macOS only
                applies new permissions on next launch.
              </p>
            </>
          )}
        </div>
      </div>

      {/* Recording toggle */}
      <ToggleRow
        label="Capture the screen when a meeting starts"
        description="When ON, hitting Start records both audio and screen into a synced mp4. Bookmarks and screenshots all key off this recording."
        checked={recordScreen}
        onChange={() => setRecordScreenPref(!recordScreen)}
        Icon={recordScreen ? Monitor : MonitorOff}
      />

      {/* Mic mux toggle */}
      <ToggleRow
        label="Include microphone audio in the screen recording"
        description="Mux your mic into the .mp4 alongside system audio. Requires macOS Microphone permission. Default OFF because permission denial yields a near-empty mp4."
        checked={recordMic}
        onChange={() => setRecordMicPref(!recordMic)}
        Icon={null}
      />

      {/* Display picker */}
      {displays.length > 0 && (
        <div className="space-y-2">
          <label className="text-sm font-medium text-gray-700 block">
            Display to record
          </label>
          <select
            value={defaultDisplayId}
            onChange={(e) => setDefaultDisplayPref(e.target.value)}
            className="w-full px-3 py-2 border border-gray-300 rounded text-sm bg-white"
          >
            <option value="">Primary display ({primaryName(displays)})</option>
            {displays.map((d) => (
              <option key={d.id} value={String(d.id)}>
                {d.name} — {d.width}×{d.height}
                {d.is_primary ? " (primary)" : ""}
              </option>
            ))}
          </select>
          <p className="text-xs text-gray-500">
            Pick a specific monitor, or leave on Primary to follow whichever display
            macOS reports as primary at start time.
          </p>
        </div>
      )}

      {/* Anthropic key cross-reference */}
      <div className="p-3 rounded-md border border-blue-200 bg-blue-50 text-sm text-blue-900">
        <p className="font-medium mb-1">Want AI captions for screenshots?</p>
        <p className="text-xs">
          The <strong>Enrich with Claude vision</strong> button on a meeting&apos;s
          Screenshots panel uses your existing Anthropic API key. Set it in{" "}
          <strong>AI Model Configuration</strong> above — pick <strong>Claude</strong>{" "}
          as the provider and paste your key. The same key powers vision enrichment;
          there&apos;s no separate field.
        </p>
      </div>
    </section>
  );
}

function primaryName(displays: Display[]): string {
  const p = displays.find((d) => d.is_primary) ?? displays[0];
  if (!p) return "none detected";
  return `${p.width}×${p.height}`;
}

function ToggleRow({
  label,
  description,
  checked,
  onChange,
  Icon,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: () => void;
  Icon: React.ComponentType<{ className?: string }> | null;
}) {
  return (
    <div className="flex items-start gap-3 py-2">
      {Icon && <Icon className="w-5 h-5 text-gray-600 mt-0.5" />}
      <div className="flex-1">
        <label className="flex items-center justify-between cursor-pointer">
          <span className="text-sm font-medium text-gray-800">{label}</span>
          <input
            type="checkbox"
            checked={checked}
            onChange={onChange}
            className="w-4 h-4 ml-3 cursor-pointer"
          />
        </label>
        <p className="text-xs text-gray-500 mt-1">{description}</p>
      </div>
    </div>
  );
}
