"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { Plus, Minus, Maximize2, X } from "lucide-react";

export type Crop = { x: number; y: number; w: number; h: number };

/**
 * Visual crop editor. Replaces the X/Y/W/H number-input UI with drag-to-draw
 * rectangle selection on the actual preview, plus zoom in/out controls.
 *
 * The image is rendered at `displayScale` (CSS pixels per source pixel).
 * `displayScale = fitScale * userZoom` where fitScale is computed so the
 * untouched image fits the viewport. The user-visible "zoom level" the
 * buttons advertise is `userZoom * 100%` — at 100% the image is shown at
 * its fit-to-viewport size, and the +/- buttons multiply userZoom.
 *
 * All coordinates round-tripped to the parent (`onChange`) are in **source
 * pixels** — independent of zoom — so the saved crop stays correct.
 */
export function CropEditor({
  dataUrl,
  naturalWidth,
  naturalHeight,
  crop,
  onChange,
  onClear,
}: {
  dataUrl: string;
  naturalWidth: number;
  naturalHeight: number;
  crop: Crop | null;
  onChange: (next: Crop | null) => void;
  onClear: () => void;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ w: 0, h: 0 });
  const [userZoom, setUserZoom] = useState(1);

  // Drag-to-draw state. Start point in source pixels.
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);

  // Measure viewport (resizes with the panel).
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      setViewport({ w: el.clientWidth, h: el.clientHeight });
    });
    ro.observe(el);
    setViewport({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const fitScale = useMemo(() => {
    if (viewport.w === 0 || viewport.h === 0 || naturalWidth === 0) return 1;
    return Math.min(viewport.w / naturalWidth, viewport.h / naturalHeight);
  }, [viewport, naturalWidth, naturalHeight]);

  const displayScale = fitScale * userZoom;
  const displayWidth = naturalWidth * displayScale;
  const displayHeight = naturalHeight * displayScale;

  // Convert pointer event → source-pixel coords on the image.
  function pointToSource(clientX: number, clientY: number): { x: number; y: number } | null {
    const el = viewportRef.current;
    if (!el || displayScale === 0) return null;
    const rect = el.getBoundingClientRect();
    // Translate viewport-local coords by the scrolled position so the image
    // origin is consistent regardless of zoom-induced overflow.
    const localX = clientX - rect.left + el.scrollLeft;
    const localY = clientY - rect.top + el.scrollTop;
    const x = clamp(Math.round(localX / displayScale), 0, naturalWidth);
    const y = clamp(Math.round(localY / displayScale), 0, naturalHeight);
    return { x, y };
  }

  function onPointerDown(e: React.PointerEvent) {
    if (e.button !== 0) return;
    const start = pointToSource(e.clientX, e.clientY);
    if (!start) return;
    e.preventDefault();
    dragStartRef.current = start;
    onChange({ x: start.x, y: start.y, w: 0, h: 0 });
    (e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: React.PointerEvent) {
    const start = dragStartRef.current;
    if (!start) return;
    const cur = pointToSource(e.clientX, e.clientY);
    if (!cur) return;
    const x = Math.min(start.x, cur.x);
    const y = Math.min(start.y, cur.y);
    const w = Math.max(1, Math.abs(cur.x - start.x));
    const h = Math.max(1, Math.abs(cur.y - start.y));
    onChange({ x, y, w, h });
  }

  function onPointerUp(e: React.PointerEvent) {
    dragStartRef.current = null;
    try {
      (e.currentTarget as HTMLDivElement).releasePointerCapture(e.pointerId);
    } catch {
      // ignored
    }
    // Tiny crops (accidental click) → drop.
    if (crop && (crop.w < 4 || crop.h < 4)) onChange(null);
  }

  function onWheel(e: React.WheelEvent) {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    setUserZoom((z) => clamp(z * factor, 0.25, 8));
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-xs">
        <button
          type="button"
          onClick={() => setUserZoom((z) => clamp(z / 1.25, 0.25, 8))}
          className="px-2 py-1 border border-gray-300 rounded hover:bg-gray-50"
          title="Zoom out"
        >
          <Minus className="w-3 h-3" />
        </button>
        <button
          type="button"
          onClick={() => setUserZoom(1)}
          className="px-2 py-1 border border-gray-300 rounded hover:bg-gray-50"
          title="Fit to window"
        >
          <Maximize2 className="w-3 h-3" />
        </button>
        <button
          type="button"
          onClick={() => setUserZoom((z) => clamp(z * 1.25, 0.25, 8))}
          className="px-2 py-1 border border-gray-300 rounded hover:bg-gray-50"
          title="Zoom in"
        >
          <Plus className="w-3 h-3" />
        </button>
        <span className="text-gray-500 ml-1">
          {Math.round(userZoom * 100)}%
        </span>
        <span className="text-gray-400 ml-2">
          {crop
            ? `Crop ${crop.w}×${crop.h} @ (${crop.x}, ${crop.y})`
            : "Drag on image to crop · ⌘/Ctrl + scroll to zoom"}
        </span>
        {crop && (
          <button
            type="button"
            onClick={onClear}
            className="ml-auto px-2 py-1 border border-gray-300 rounded hover:bg-gray-50 flex items-center gap-1 text-gray-600"
            title="Clear crop"
          >
            <X className="w-3 h-3" /> Clear
          </button>
        )}
      </div>

      <div
        ref={viewportRef}
        onWheel={onWheel}
        className="relative w-full bg-gray-900 rounded overflow-auto cursor-crosshair select-none"
        style={{ aspectRatio: `${naturalWidth} / ${naturalHeight}`, maxHeight: 360 }}
      >
        <div
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          style={{
            position: "relative",
            width: displayWidth || "100%",
            height: displayHeight || "100%",
          }}
        >
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={dataUrl}
            alt="frame"
            draggable={false}
            style={{
              width: "100%",
              height: "100%",
              display: "block",
              objectFit: "contain",
              userSelect: "none",
              pointerEvents: "none",
            }}
          />
          {/* Crop overlay */}
          {crop && displayScale > 0 && (
            <>
              <div
                style={{
                  position: "absolute",
                  inset: 0,
                  background: "rgba(0,0,0,0.45)",
                  pointerEvents: "none",
                  clipPath: `polygon(
                    0 0, 0 100%, ${crop.x * displayScale}px 100%,
                    ${crop.x * displayScale}px ${crop.y * displayScale}px,
                    ${(crop.x + crop.w) * displayScale}px ${crop.y * displayScale}px,
                    ${(crop.x + crop.w) * displayScale}px ${(crop.y + crop.h) * displayScale}px,
                    ${crop.x * displayScale}px ${(crop.y + crop.h) * displayScale}px,
                    ${crop.x * displayScale}px 100%,
                    100% 100%, 100% 0
                  )`,
                }}
              />
              <div
                style={{
                  position: "absolute",
                  left: crop.x * displayScale,
                  top: crop.y * displayScale,
                  width: crop.w * displayScale,
                  height: crop.h * displayScale,
                  border: "2px solid #3b82f6",
                  boxShadow: "0 0 0 1px rgba(0,0,0,0.4) inset",
                  pointerEvents: "none",
                }}
              />
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}
