// Click-follow camera, pointer track and ripples, computed from a clip's events.json.
// All times are in seconds of clip time; positions in video pixels (1920x1008).

export type Ev = {
  t_ms: number; type: string; x: number; y: number; target?: string; zoom?: number; focus?: boolean;
  ms?: number; amount?: number;
};
export type Clip = { scene: string; region: number[]; duration_ms: number; events: Ev[] };
export type Cam = { z: number; cx: number; cy: number };

export const W = 1920;
export const H = 1008;

const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
// Critically-damped-ish spring (no overshoot past ~2 %): 0.5 s settle.
const springish = (t: number) => {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  const w = 9.5;
  return 1 - Math.exp(-w * t) * (1 + w * t); // critically damped: no overshoot, settled at t = 1
};

type Move = { t0: number; dur: number; from: Cam; to: Cam; kind: "spring" | "ease" };

const clampCam = (c: Cam): Cam => {
  const hw = W / (2 * c.z);
  const hh = H / (2 * c.z);
  return { z: c.z, cx: Math.min(W - hw, Math.max(hw, c.cx)), cy: Math.min(H - hh, Math.max(hh, c.cy)) };
};

/** Camera moves from the focus events (clicks, keys, moves marked focus) with a zoom level. */
export function cameraPlan(clip: Clip, maxZoom: number, zoomScale = 1): Move[] {
  const focus = clip.events
    .filter((e) => e.zoom && (e.type === "click" || e.type === "key" || e.focus))
    .map((e) => ({ t: e.t_ms / 1000, x: e.x, y: e.y, z: Math.min(maxZoom, 1 + (e.zoom! - 1) * zoomScale) }));
  const moves: Move[] = [];
  let cur: Cam = { z: 1, cx: W / 2, cy: H / 2 };
  const push = (t0: number, dur: number, to: Cam, kind: Move["kind"]) => {
    const c = clampCam(to);
    moves.push({ t0, dur, from: cur, to: c, kind });
    cur = c;
  };
  for (let i = 0; i < focus.length; i++) {
    const f = focus[i];
    const prev = focus[i - 1];
    const next = focus[i + 1];
    const near = prev && (f.t - prev.t < 2 || Math.hypot(f.x - prev.x, f.y - prev.y) < 300);
    if (near && cur.z > 1) {
      // Same region or soon after: keep the zoom, pan across.
      push(Math.max(prev.t + 0.05, f.t - 0.45), 0.45, { z: Math.max(cur.z, f.z), cx: f.x, cy: f.y }, "ease");
    } else {
      push(Math.max(0, f.t - 0.55), 0.5, { z: f.z, cx: f.x, cy: f.y }, "spring");
    }
    const nextNear = next && (next.t - f.t < 2 || Math.hypot(next.x - f.x, next.y - f.y) < 300);
    if (!nextNear) {
      const out = next ? Math.min(f.t + 1.4, next.t - 1.2) : f.t + 1.6;
      if (!next || out > f.t + 0.3) push(Math.max(f.t + 0.3, out), 0.6, { z: 1, cx: W / 2, cy: H / 2 }, "ease");
    }
  }
  return moves;
}

export function cameraAt(moves: Move[], t: number): Cam {
  let c: Cam = { z: 1, cx: W / 2, cy: H / 2 };
  for (const m of moves) {
    if (t < m.t0) break;
    const p = Math.min(1, (t - m.t0) / m.dur);
    const k = m.kind === "spring" ? springish(p) : easeInOut(p);
    c = { z: m.from.z + (m.to.z - m.from.z) * k, cx: m.from.cx + (m.to.cx - m.from.cx) * k, cy: m.from.cy + (m.to.cy - m.from.cy) * k };
  }
  return c;
}

/** Pointer position from the move/click events (linear between samples, which are 60 Hz). */
export function pointerAt(clip: Clip, t: number): { x: number; y: number } {
  const pts = clip.events.filter((e) => e.type === "move" || e.type === "click" || e.type === "hold" || e.type === "scroll");
  if (!pts.length) return { x: W / 2, y: H / 2 };
  const ms = t * 1000;
  if (ms <= pts[0].t_ms) return { x: pts[0].x, y: pts[0].y };
  for (let i = 1; i < pts.length; i++) {
    if (pts[i].t_ms >= ms) {
      const a = pts[i - 1], b = pts[i];
      const k = b.t_ms === a.t_ms ? 1 : (ms - a.t_ms) / (b.t_ms - a.t_ms);
      return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
    }
  }
  const l = pts[pts.length - 1];
  return { x: l.x, y: l.y };
}

/** Keep the pointer inside the visible part of a zoomed view (never let the action leave frame). */
export function follow(c: Cam, p: { x: number; y: number }): Cam {
  if (c.z <= 1.001) return c;
  const hw = (W / (2 * c.z)) * 0.86;
  const hh = (H / (2 * c.z)) * 0.86;
  let { cx, cy } = c;
  if (p.x < cx - hw) cx = p.x + hw;
  if (p.x > cx + hw) cx = p.x - hw;
  if (p.y < cy - hh) cy = p.y + hh;
  if (p.y > cy + hh) cy = p.y - hh;
  return clampCam({ z: c.z, cx, cy });
}
