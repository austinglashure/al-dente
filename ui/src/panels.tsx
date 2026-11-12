// Side-panel sizes and visibility, remembered per browser. Storage can be
// unavailable (private windows, blocked site data); then defaults are used.

import { useCallback, useEffect, useState } from 'react';

const KEY = 'aldente-panels';
const MIN = 180;
const MAX = 720;

export interface Panels {
  left: number;
  right: number;
  leftOpen: boolean;
  rightOpen: boolean;
}

const DEFAULTS: Panels = { left: 280, right: 360, leftOpen: true, rightOpen: true };

/** A width kept between the minimum and maximum.
 *  Example: `clampWidth(50)` is `180`; `clampWidth(400)` is `400`. */
export function clampWidth(w: number): number {
  return Math.min(MAX, Math.max(MIN, Math.round(w)));
}

/** Reads saved panels, falling back to defaults for anything missing or invalid.
 *  Example: `parsePanels('{"left": 300}')` keeps the default right width. */
export function parsePanels(raw: string | null): Panels {
  try {
    const p = JSON.parse(raw ?? '{}') as Partial<Panels>;
    return {
      left: typeof p.left === 'number' ? clampWidth(p.left) : DEFAULTS.left,
      right: typeof p.right === 'number' ? clampWidth(p.right) : DEFAULTS.right,
      leftOpen: typeof p.leftOpen === 'boolean' ? p.leftOpen : DEFAULTS.leftOpen,
      rightOpen: typeof p.rightOpen === 'boolean' ? p.rightOpen : DEFAULTS.rightOpen,
    };
  } catch {
    return DEFAULTS;
  }
}

/** Panel state that persists to localStorage when it can.
 *  Example: `const [panels, setPanels] = usePanels();`. */
export function usePanels(): [Panels, (change: Partial<Panels>) => void] {
  const [panels, set] = useState<Panels>(() => {
    try {
      return parsePanels(window.localStorage.getItem(KEY));
    } catch {
      return DEFAULTS;
    }
  });
  useEffect(() => {
    try {
      window.localStorage.setItem(KEY, JSON.stringify(panels));
    } catch {
      // Not saved; the layout still works for this visit.
    }
  }, [panels]);
  const update = useCallback((change: Partial<Panels>) => set((p) => ({ ...p, ...change })), []);
  return [panels, update];
}

/** A draggable border that resizes a panel; arrow keys work when focused.
 *  `side` says which panel it resizes, so dragging toward the canvas widens it.
 *  Example: `<Splitter side="right" width={360} onResize={(w) => …} />`. */
export function Splitter({ side, width, onResize }: { side: 'left' | 'right'; width: number; onResize: (w: number) => void }) {
  const start = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const x0 = e.clientX;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - x0;
      onResize(clampWidth(side === 'left' ? width + dx : width - dx));
    };
    const end = () => {
      target.removeEventListener('pointermove', move);
      target.removeEventListener('pointerup', end);
    };
    target.addEventListener('pointermove', move);
    target.addEventListener('pointerup', end);
  };
  const key = (e: React.KeyboardEvent) => {
    const step = e.shiftKey ? 60 : 20;
    const grow = side === 'left' ? 'ArrowRight' : 'ArrowLeft';
    const shrink = side === 'left' ? 'ArrowLeft' : 'ArrowRight';
    if (e.key === grow) onResize(clampWidth(width + step));
    if (e.key === shrink) onResize(clampWidth(width - step));
  };
  return (
    <div
      className="splitter"
      role="separator"
      aria-orientation="vertical"
      aria-label={`Resize the ${side} panel`}
      aria-valuenow={width}
      tabIndex={0}
      data-testid={`resize-${side}`}
      onPointerDown={start}
      onKeyDown={key}
    />
  );
}
