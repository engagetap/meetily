/**
 * True only when running inside the Tauri webview. Anything that calls
 * `invoke()` from `@tauri-apps/api/core` will throw a confusing
 * `transformCallback` error if hit in a regular browser tab — guard
 * with this before invoking.
 */
export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !==
      "undefined"
  );
}
