/**
 * True only when running inside the Tauri webview. Anything that calls
 * `invoke()` from `@tauri-apps/api/core` will throw a confusing
 * `transformCallback` error if hit in a regular browser tab — guard
 * with this before invoking.
 */
export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    // @ts-expect-error -- Tauri injects this at runtime
    typeof window.__TAURI_INTERNALS__ !== "undefined"
  );
}
