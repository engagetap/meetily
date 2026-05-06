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

    const primary = displays.find((d) => d.is_primary) ?? displays[0];
    await invoke('screen_start_recording', {
      meetingId: `screen-${crypto.randomUUID()}`,
      displayId: primary.id,
      fps: 30,
      bitrateKbps: 3000,
      captureMic,
    });
    console.log('Screen recording started');
  } catch (err) {
    console.warn('Screen recording skipped:', err);
    toast.error('Screen recording failed', {
      description: String(err),
      duration: 6000,
    });
  }
}

/**
 * Stops the screen recorder if one is running. No-op otherwise. Same
 * fail-soft contract as start.
 */
export async function maybeStopScreenRecording(): Promise<void> {
  try {
    const stillRecording = await invoke<boolean>('screen_is_recording');
    if (stillRecording) {
      await invoke('screen_stop_recording');
      console.log('Screen recording stopped');
    }
  } catch (err) {
    console.warn('Screen stop skipped:', err);
  }
}
