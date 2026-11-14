import React from "react";
import { AbsoluteFill, OffthreadVideo, interpolate, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import { Clip, H, W, cameraAt, cameraPlan, follow, pointerAt } from "./camera";
import { CARD, T } from "./theme";

export type SceneSpec = { clip: string; from: number; beats: number; caption: string; captionAt?: number };

const Pointer: React.FC<{ x: number; y: number; scale: number }> = ({ x, y, scale }) => (
  // A plain arrow, drawn 1.5x the system pointer, with a soft shadow.
  <svg width={26 * scale} height={38 * scale} viewBox="0 0 26 38"
    style={{ position: "absolute", left: x - 2 * scale, top: y - 2 * scale, filter: "drop-shadow(0 3px 6px rgba(0,0,0,.45))" }}>
    <path d="M2 2 L2 30 L9.5 23.5 L14.5 35 L19 33 L14 21.5 L24 21.5 Z" fill="#fff" stroke="#111" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);

export const Scene: React.FC<{ spec: SceneSpec; data: Clip; maxZoom: number; zoomScale: number; durFrames: number }> = ({
  spec, data, maxZoom, zoomScale, durFrames,
}) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const t = spec.from + frame / fps; // clip time
  const moves = React.useMemo(() => cameraPlan(data, maxZoom, zoomScale), [data, maxZoom, zoomScale]);
  const p = pointerAt(data, t);
  const cam = follow(cameraAt(moves, t), p);
  const s0 = CARD.w / W;
  // Video px -> card px under the camera.
  const toCard = (x: number, y: number) => ({ x: ((x - cam.cx) * cam.z + W / 2) * s0, y: ((y - cam.cy) * cam.z + H / 2) * s0 });
  const pc = toCard(p.x, p.y);
  const showPointer = data.scene !== "01-problem" || true;

  // Ripples at clicks (0.5 s), key labels (1.4 s).
  const clicks = data.events.filter((e) => e.type === "click");
  const keys = data.events.filter((e) => e.type === "key");

  // Caption: 0.3 s before the scene's first action, out at the end.
  const firstAction = data.events.find((e) => e.zoom)?.t_ms ?? 0;
  const capIn = spec.captionAt ?? Math.max(0.15, firstAction / 1000 - spec.from - 0.3);
  const capF = frame - capIn * fps;
  const capO = interpolate(capF, [0, 9], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) *
    interpolate(frame, [durFrames - 8, durFrames - 1], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const capY = interpolate(capF, [0, 12], [14, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });

  return (
    <AbsoluteFill>
      <div style={{ position: "absolute", top: 40, width: "100%", textAlign: "center", opacity: capO, transform: `translateY(${capY}px)` }}>
        <span style={{ font: `600 76px ${T.font}`, color: T.text, letterSpacing: "-1px" }}>{spec.caption}</span>
        <span style={{ display: "inline-block", width: 14, height: 14, borderRadius: 7, background: T.clay, marginLeft: 16, verticalAlign: "middle" }} />
      </div>
      <div style={{ position: "absolute", left: CARD.x, top: CARD.y, width: CARD.w, height: CARD.h, borderRadius: 12, overflow: "hidden",
        boxShadow: "0 30px 80px rgba(0,0,0,.55), 0 0 0 1px rgba(255,255,255,.06)", background: "#14171c" }}>
        <div style={{ position: "absolute", left: 0, top: 0, width: W, height: H, transformOrigin: "0 0",
          transform: `scale(${s0}) translate(${W / 2}px, ${H / 2}px) scale(${cam.z}) translate(${-cam.cx}px, ${-cam.cy}px)` }}>
          <OffthreadVideo src={staticFile(`clips/${spec.clip}.mp4`)} startFrom={Math.round(spec.from * fps)} muted
            style={{ width: W, height: H }} />
        </div>
        {clicks.map((c, i) => {
          const dt = t - c.t_ms / 1000;
          if (dt < 0 || dt > 0.55) return null;
          const q = toCard(c.x, c.y);
          const k = dt / 0.55;
          const r = 10 + 46 * (1 - Math.pow(1 - k, 3));
          return <div key={i} style={{ position: "absolute", left: q.x - r, top: q.y - r, width: 2 * r, height: 2 * r, borderRadius: r,
            border: `3px solid ${T.clay}`, opacity: 0.85 * (1 - k), background: `rgba(210,117,86,${0.18 * (1 - k)})` }} />;
        })}
        {showPointer && <Pointer x={pc.x} y={pc.y} scale={1.5} />}
        {keys.map((k, i) => {
          const dt = t - k.t_ms / 1000;
          if (dt < -0.05 || dt > 1.5) return null;
          const o = interpolate(dt, [-0.05, 0.1, 1.2, 1.5], [0, 1, 1, 0]);
          return <div key={i} style={{ position: "absolute", left: 0, right: 0, bottom: 40, textAlign: "center", opacity: o }}>
            <span style={{ font: `600 30px ${T.font}`, color: T.text, background: "rgba(20,23,28,.88)", padding: "12px 24px",
              borderRadius: 12, border: `1px solid ${T.hair}`, boxShadow: "0 8px 30px rgba(0,0,0,.4)" }}>{k.target}</span>
          </div>;
        })}
      </div>
    </AbsoluteFill>
  );
};
