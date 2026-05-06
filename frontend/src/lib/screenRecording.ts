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

    // Honour the user's default-display preference if set; otherwise the
    // OS-reported primary display.
    const preferredId = parseInt(localStorage.getItem('meetily.defaultDisplayId') ?? '', 10);
    const chosen =
      (Number.isFinite(preferredId) && displays.find((d) => d.id === preferredId)) ||
      displays.find((d) => d.is_primary) ||
      displays[0];

    await invoke('screen_start_recording', {
      meetingId: `screen-${crypto.randomUUID()}`,
      displayId: chosen.id,
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
            description: 'Open the meeting to review them.',
            duration: 5000,
          });
        }
      } catch (genErr) {
        console.warn('Auto-generate failed:', genErr);
      }
    }
  } catch (err) {
    console.warn('Screen stop skipped:', err);
  }
}
