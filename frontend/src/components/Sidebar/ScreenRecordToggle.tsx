"use client";

import { Monitor, MonitorOff } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  openScreenRecordingSettings,
  requestScreenRecordingPermission,
  setRecordScreenPref,
} from "@/lib/screenRecording";
import { useScreenRecordingPrefs } from "@/hooks/useScreenRecordingPrefs";
import { toast } from "sonner";

/**
 * Sidebar toggle for the screen-recording feature. Shown next to the
 * Start Recording button. Reads + writes the shared
 * `meetily.recordScreen` pref via useScreenRecordingPrefs so the value
 * stays in sync with the Settings panel toggle.
 */
export function ScreenRecordToggle({ disabled = false }: { disabled?: boolean }) {
  const { recordScreen: enabled } = useScreenRecordingPrefs();

  async function toggle() {
    const next = !enabled;
    setRecordScreenPref(next);
    if (next) {
      // Trigger the OS prompt the moment the user opts in. If macOS has
      // never been asked, this raises the TCC dialog. If a previous
      // decision is on file, it's a no-op and we fall through to checking
      // the resulting state.
      const granted = await requestScreenRecordingPermission();
      if (!granted) {
        toast.error("Screen Recording permission required", {
          description:
            "macOS denied screen-capture access. Grant it in System Settings → Privacy & Security → Screen & System Audio Recording, then restart the app.",
          duration: 10000,
          action: {
            label: "Open Settings",
            onClick: () => {
              void openScreenRecordingSettings();
            },
          },
        });
      }
    }
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          onClick={toggle}
          disabled={disabled}
          className={`p-2 rounded-full transition-colors duration-150 shadow-sm ${
            enabled
              ? "bg-blue-500 hover:bg-blue-600 text-white"
              : "bg-gray-200 hover:bg-gray-300 text-gray-700"
          } ${disabled ? "opacity-50 cursor-not-allowed" : ""}`}
        >
          {enabled ? (
            <Monitor className="w-5 h-5" />
          ) : (
            <MonitorOff className="w-5 h-5" />
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent side="right">
        <p>
          {enabled
            ? "Screen recording: ON"
            : "Screen recording: OFF (audio only)"}
        </p>
      </TooltipContent>
    </Tooltip>
  );
}
