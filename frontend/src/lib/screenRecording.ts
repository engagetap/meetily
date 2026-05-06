/**
 * Helper used by every code path that starts an audio recording so screen
 * recording stays in lock-step. Honors the localStorage flags
 * `meetily.recordScreen` (default ON) and `meetily.recordMic` (default OFF).
 *
 * Fail-soft: returns silently on any failure (no display, missing
 * permission, command rejected) — the audio recording the user actually
 * asked for must never be aborted by a screen-record problem. But we
 * surface a toast when permission is the problem so the user knows.
 */
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

export async function hasScreenRecordingPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('screen_has_permission');
  } catch {
    return false;
  }
}

/**
 * Trigger the macOS Screen Recording TCC prompt if it hasn't been answered
 * yet. Returns the post-prompt grant state. Equivalent of meetily's
 * `trigger_microphone_permission` for the audio side — same shape.
 */
export async function requestScreenRecordingPermission(): Promise<boolean> {
  try {
    return await invoke<boolean>('screen_request_permission');
  } catch (e) {
    console.warn('screen_request_permission failed:', e);
    return false;
  }
}

export async function openScreenRecordingSettings(): Promise<void> {
  try {
    await invoke('screen_open_permission_settings');
  } catch (e) {
    console.warn('Could not open settings:', e);
  }
}

export async function maybeStartScreenRecording(): Promise<void> {
  if (typeof window === 'undefined') return;

  const screenEnabled = localStorage.getItem('meetily.recordScreen') !== 'false';
  if (!screenEnabled) return;

  const captureMic = localStorage.getItem('meetily.recordMic') === 'true';

  // Fire the TCC prompt up front (no-op if already answered). Without this
  // first-time users on a fresh install never see the OS dialog because
  // SCStream alone doesn't reliably surface it from a Tokio worker thread.
  await requestScreenRecordingPermission();

  try {
    const displays = await invoke<Array<{ id: number; is_primary: boolean }>>(
      'screen_list_displays'
    ).catch(() => [] as Array<{ id: number; is_primary: boolean }>);

    if (displays.length === 0) {
      console.warn('Screen recording skipped: no displays (permission denied?)');
      toast.error('Screen recording permission required', {
        description:
          'Grant Meetily access in System Settings → Privacy & Security → Screen & System Audio Recording, then restart the app.',
        duration: 8000,
        action: {
          label: 'Open Settings',
          onClick: () => {
            void openScreenRecordingSettings();
          },
        },
      });
      return;
    }

    // Ask the user which display to record. The DisplayPickerOverlay
    // (mounted in app/layout) registers a window-global resolver. The
    // overlay short-circuits silently when there's only one display, when
    // the user opted out via "Ask before every recording", or when no
    // saved default exists. A `null` return means the user cancelled —
    // skip screen recording entirely.
    let chosenId: number | null = null;
    if (window.__meetilyDisplayPicker) {
      chosenId = await window.__meetilyDisplayPicker.pick();
    } else {
      // Fallback for environments without the overlay (e.g. local API).
      const preferredId = parseInt(localStorage.getItem('meetily.defaultDisplayId') ?? '', 10);
      const chosen =
        (Number.isFinite(preferredId) && displays.find((d) => d.id === preferredId)) ||
        displays.find((d) => d.is_primary) ||
        displays[0];
      chosenId = chosen?.id ?? null;
    }

    if (chosenId == null) {
      console.log('Display picker cancelled — skipping screen recording');
      return;
    }

    await invoke('screen_start_recording', {
      meetingId: `screen-${crypto.randomUUID()}`,
      displayId: chosenId,
      fps: 30,
      bitrateKbps: 3000,
      captureMic,
    });
    console.log('Screen recording started');
  } catch (err) {
    console.warn('Screen recording skipped:', err);
    toast.error('Screen recording failed', {
      description: formatTauriError(err),
      duration: 8000,
      action: {
        label: 'Open Settings',
        onClick: () => void openScreenRecordingSettings(),
      },
    });
  }
}

/**
 * Auto-captions pending screenshots in the background after a recording stops.
 *
 * Two paths, run in this order:
 *   1. Vision (Claude) — highest quality, also receives a transcript window
 *      around each screenshot's timestamp so the caption reflects both
 *      what's visible and what was being said. Skipped if no Anthropic
 *      key is configured.
 *   2. Transcript-only fallback — for any screenshot whose caption is
 *      still empty, picks the closest transcript segment and trims it
 *      to a short caption. Local, no API needed. Waits up to 60s for
 *      transcripts to finish processing.
 */
async function autoCaptionInBackground(meetingId: string): Promise<void> {
  // 1. Vision (best effort — runs even without transcripts).
  try {
    const result = await invoke<{
      processed: number;
      updated: number;
      errors: string[];
    }>('screenshots_enrich_with_vision', { meetingId });
    if (result.updated > 0) {
      console.log(`Auto-captioned ${result.updated} screenshot(s) with Claude vision`);
    }
  } catch (e) {
    console.warn('Auto vision-caption skipped:', e);
  }

  // 2. Transcript-only fallback. Transcription is asynchronous; poll a
  // few times so we catch newly-finalized segments without blocking.
  for (let attempt = 0; attempt < 6; attempt++) {
    await new Promise((r) => setTimeout(r, 5000));
    try {
      const r = await invoke<{
        processed: number;
        captioned: number;
      }>('screenshots_caption_from_transcript', { meetingId });
      if (r.captioned > 0) {
        console.log(`Auto-captioned ${r.captioned} screenshot(s) from transcript`);
        break;
      }
      if (r.processed === 0) {
        // No remaining pending screenshots — we're done.
        break;
      }
    } catch (e) {
      console.warn('Transcript caption attempt failed:', e);
      break;
    }
  }
}

/**
 * Tauri command errors come back as objects shaped like
 * `{ type: 'PermissionDenied' }` or `{ type: 'Internal', message: '...' }`,
 * which `String()` flattens into `[object Object]`. This formats them
 * into something readable.
 */
function formatTauriError(err: unknown): string {
  if (err == null) return 'Unknown error';
  if (typeof err === 'string') return err;
  if (typeof err === 'object') {
    const e = err as { type?: string; message?: string };
    if (e.message && e.type) return `${e.type}: ${e.message}`;
    if (e.message) return e.message;
    if (e.type) return e.type;
    try {
      return JSON.stringify(err);
    } catch {
      return 'Unknown error object';
    }
  }
  return String(err);
}

/**
 * Custom event broadcast whenever a screen-recording preference is
 * changed via setRecordScreenPref / setRecordMicPref / setDefaultDisplayPref.
 * Components subscribed via `useScreenRecordingPrefs` re-read localStorage
 * when this fires, so toggles in the sidebar and the Settings panel stay
 * in sync.
 */
export const PREFS_EVENT = 'meetily.screen-recording-prefs-changed';

export function setRecordScreenPref(value: boolean): void {
  if (typeof window === 'undefined') return;
  localStorage.setItem('meetily.recordScreen', value ? 'true' : 'false');
  window.dispatchEvent(new Event(PREFS_EVENT));
}

export function setRecordMicPref(value: boolean): void {
  if (typeof window === 'undefined') return;
  localStorage.setItem('meetily.recordMic', value ? 'true' : 'false');
  window.dispatchEvent(new Event(PREFS_EVENT));
}

export function setDefaultDisplayPref(displayId: string): void {
  if (typeof window === 'undefined') return;
  if (displayId === '') {
    localStorage.removeItem('meetily.defaultDisplayId');
  } else {
    localStorage.setItem('meetily.defaultDisplayId', displayId);
  }
  window.dispatchEvent(new Event(PREFS_EVENT));
}

export function getRecordScreenPref(): boolean {
  if (typeof window === 'undefined') return true;
  return localStorage.getItem('meetily.recordScreen') !== 'false';
}

export function getRecordMicPref(): boolean {
  if (typeof window === 'undefined') return false;
  return localStorage.getItem('meetily.recordMic') === 'true';
}

export function getDefaultDisplayPref(): string {
  if (typeof window === 'undefined') return '';
  return localStorage.getItem('meetily.defaultDisplayId') ?? '';
}

type RecordingMeta = {
  file_path: string;
  width: number;
  height: number;
  fps: number;
  codec: string;
  display_id: number;
  duration_ms: number;
  meeting_id?: string | null;
};

/**
 * Stops the screen recorder if one is running. After a successful stop,
 * auto-generates screenshot candidates (bookmarks + frame-diff) for the
 * meeting so the user lands on the meeting-details page with a populated
 * Screenshots panel — no manual Generate click required.
 *
 * Fail-soft on every step.
 */
export async function maybeStopScreenRecording(): Promise<void> {
  try {
    const stillRecording = await invoke<boolean>('screen_is_recording');
    if (!stillRecording) return;

    const meta = await invoke<RecordingMeta>('screen_stop_recording');
    console.log('Screen recording stopped:', meta);

    if (meta.meeting_id) {
      try {
        const count = await invoke<number>('screenshots_generate', {
          meetingId: meta.meeting_id,
        });
        console.log(`Auto-generated ${count} screenshot candidate(s)`);
        if (count > 0) {
          toast.success(`Generated ${count} screenshot candidate${count === 1 ? '' : 's'}`, {
            description: 'Captioning in the background — open the meeting to review.',
            duration: 5000,
          });
        }
      } catch (genErr) {
        console.warn('Auto-generate failed:', genErr);
      }

      // Auto-caption: kick this off in the background so the user lands on
      // the meeting-details page with captions already populated. Vision
      // (Claude) gives the highest quality but needs a key + transcript
      // takes a beat to finish, so we fire-and-forget. The Screenshots
      // panel will pick the captions up on its next refresh.
      void autoCaptionInBackground(meta.meeting_id);
    }
  } catch (err) {
    console.warn('Screen stop skipped:', err);
  }
}
