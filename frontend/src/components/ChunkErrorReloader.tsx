"use client";

import { useEffect } from "react";

/**
 * Catches webpack `ChunkLoadError` (the dev-server hot-reload tripping on
 * stale chunk hashes) and force-reloads the window so the user doesn't
 * have to dig out ⌘R inside the Tauri webview. Lives in the root layout.
 *
 * Production builds don't hit this code path because chunks are
 * content-hashed and don't go missing.
 */
export function ChunkErrorReloader() {
  useEffect(() => {
    let reloading = false;

    function reloadOnce(reason: unknown) {
      if (reloading) return;
      reloading = true;
      console.warn("ChunkErrorReloader: reloading after", reason);
      // Small delay so the console message is visible before the page tears down.
      setTimeout(() => window.location.reload(), 50);
    }

    function looksLikeChunkError(err: unknown): boolean {
      if (!err) return false;
      const e = err as { name?: string; message?: string };
      const name = e.name ?? "";
      const message = e.message ?? "";
      return (
        name.includes("ChunkLoadError") ||
        message.includes("ChunkLoadError") ||
        message.includes("Loading chunk") ||
        message.includes("Loading CSS chunk")
      );
    }

    function onError(event: ErrorEvent) {
      if (looksLikeChunkError(event.error ?? event.message)) {
        reloadOnce(event.error ?? event.message);
      }
    }
    function onRejection(event: PromiseRejectionEvent) {
      if (looksLikeChunkError(event.reason)) {
        reloadOnce(event.reason);
      }
    }

    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, []);

  return null;
}
