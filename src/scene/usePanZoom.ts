import { useEffect, useRef, useState } from "react";
import type { MouseEvent, PointerEvent, RefObject } from "react";

export interface View {
  readonly zoom: number;
  readonly x: number;
  readonly y: number;
}

export interface PanZoom {
  readonly view: View;
  readonly dragging: boolean;
  /** Zoom by a factor around the centre of the stage. */
  readonly zoomBy: (factor: number) => void;
  readonly reset: () => void;
  readonly handlers: {
    readonly onPointerDown: (e: PointerEvent<HTMLElement>) => void;
    readonly onPointerMove: (e: PointerEvent<HTMLElement>) => void;
    readonly onPointerUp: (e: PointerEvent<HTMLElement>) => void;
    readonly onPointerCancel: (e: PointerEvent<HTMLElement>) => void;
    readonly onClickCapture: (e: MouseEvent<HTMLElement>) => void;
  };
}

export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 4;
const FIT: View = { zoom: 1, x: 0, y: 0 };
/** How far the pointer travels before a press becomes a drag rather than a click. */
const DRAG_SLOP = 4;

const clamp = (z: number): number => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

/** Zoom by `factor` keeping the stage point (px, py) still on screen. */
export function zoomAround(view: View, factor: number, px: number, py: number): View {
  const zoom = clamp(view.zoom * factor);
  const k = zoom / view.zoom;
  return { zoom, x: px - (px - view.x) * k, y: py - (py - view.y) * k };
}

/**
 * Free zoom and pan over a fitted stage: scrolling (two fingers on a trackpad) zooms
 * at the pointer, dragging pans. The view is applied as translate(x, y) scale(zoom)
 * in the stage's own pixels, on top of the fit.
 */
export function usePanZoom(ref: RefObject<HTMLElement | null>, enabled: boolean): PanZoom {
  const [view, setView] = useState<View>(FIT);
  const [dragging, setDragging] = useState(false);
  const press = useRef<{ readonly id: number; readonly startX: number; readonly startY: number; x: number; y: number; moved: boolean } | null>(null);
  const swallowClick = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!enabled || el === null) return;
    const local = (clientX: number, clientY: number): { x: number; y: number } => {
      const r = el.getBoundingClientRect();
      return { x: clientX - r.left, y: clientY - r.top };
    };
    const onWheel = (e: WheelEvent): void => {
      e.preventDefault();
      const p = local(e.clientX, e.clientY);
      // A trackpad swipe sends many small deltas, a mouse notch one of ~100px: cap so a notch is one steady step.
      const delta = Math.max(-50, Math.min(50, e.deltaY));
      setView((v) => zoomAround(v, Math.exp(-delta * 0.003), p.x, p.y));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      el.removeEventListener("wheel", onWheel);
    };
  }, [ref, enabled]);

  const release = (e: PointerEvent<HTMLElement>): void => {
    if (press.current?.id !== e.pointerId) return;
    swallowClick.current = press.current.moved;
    press.current = null;
    setDragging(false);
  };

  return {
    view,
    dragging,
    zoomBy: (factor) => {
      const el = ref.current;
      if (el !== null) setView((v) => zoomAround(v, factor, el.clientWidth / 2, el.clientHeight / 2));
    },
    reset: () => setView(FIT),
    handlers: {
      onPointerDown: (e) => {
        swallowClick.current = false;
        if (!enabled || e.button !== 0) return;
        press.current = { id: e.pointerId, startX: e.clientX, startY: e.clientY, x: e.clientX, y: e.clientY, moved: false };
      },
      onPointerMove: (e) => {
        const p = press.current;
        if (p?.id !== e.pointerId) return;
        if (!p.moved) {
          if (Math.hypot(e.clientX - p.startX, e.clientY - p.startY) < DRAG_SLOP) return;
          p.moved = true;
          e.currentTarget.setPointerCapture(e.pointerId);
          setDragging(true);
        }
        const dx = e.clientX - p.x;
        const dy = e.clientY - p.y;
        p.x = e.clientX;
        p.y = e.clientY;
        setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }));
      },
      onPointerUp: release,
      onPointerCancel: release,
      onClickCapture: (e) => {
        // A drag that ends over a desk must not also open it.
        if (!swallowClick.current) return;
        swallowClick.current = false;
        e.preventDefault();
        e.stopPropagation();
      },
    },
  };
}
