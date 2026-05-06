"use client";

import { useEffect, useState } from "react";
import {
  PREFS_EVENT,
  getDefaultDisplayPref,
  getRecordMicPref,
  getRecordScreenPref,
} from "@/lib/screenRecording";

/**
 * Returns the current screen-recording preferences and re-renders whenever
 * any of them change — even when the change came from a different
 * component on the same page (sidebar toggle vs. Settings panel etc.).
 *
 * Backed by localStorage + a custom-event broadcast (PREFS_EVENT) so we
 * don't need a context provider just for these few flags.
 */
export function useScreenRecordingPrefs() {
  const [recordScreen, setRecordScreen] = useState<boolean>(true);
  const [recordMic, setRecordMic] = useState<boolean>(false);
  const [defaultDisplayId, setDefaultDisplayId] = useState<string>("");

  useEffect(() => {
    const refresh = () => {
      setRecordScreen(getRecordScreenPref());
      setRecordMic(getRecordMicPref());
      setDefaultDisplayId(getDefaultDisplayPref());
    };
    refresh();
    window.addEventListener(PREFS_EVENT, refresh);
    // Cross-tab updates (the Tauri webview doesn't have multiple tabs but
    // it's free): listen for native `storage` events too.
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(PREFS_EVENT, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);

  return { recordScreen, recordMic, defaultDisplayId };
}
