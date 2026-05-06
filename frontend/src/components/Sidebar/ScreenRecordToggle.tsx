"use client";

import { useEffect, useState } from "react";
import { Monitor, MonitorOff } from "lucide-react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  hasScreenRecordingPermission,
  openScreenRecordingSettings,
} from "@/lib/screenRecording";
import { toast } from "sonner";

const STORAGE_KEY = "meetily.recordScreen";

/**
 * Sidebar toggle for the screen-recording feature. Shown next to the
 * Start Recording button. When the user flips it ON, we proactively
 * check macOS Screen Recording permission so they don't hit a silent
 * failure later.
 */
export function ScreenRecordToggle({ disabled = false }: { disabled?: boolean }) {
  const [enabled, setEnabled] = useState(true);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const saved = localStorage.getItem(STORAGE_KEY);
    setEnabled(saved === null ? true : saved !== "false");
  }, []);

  async function toggle() {
    const next = !enabled;
    setEnabled(next);
    if (typeof window !== "undefined") {
      localStorage.setItem(STORAGE_KEY, next ? "true" : "false");
    }
    if (next) {
      // Verify permission so the user doesn't discover the problem mid-meeting.
      const granted = await hasScreenRecordingPermission();
      if (!granted) {
        toast.error("Screen Recording permission required", {
          description:
            "Meetily needs Screen Recording access to capture your screen. Grant it in System Settings, then restart the app.",
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
