import React from "react";
import { AbsoluteFill, Audio, Img, Sequence, interpolate, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import beats from "./beats.json";
import { Clip } from "./camera";
import { Scene, SceneSpec } from "./Scene";
import { T } from "./theme";

const clips: Record<string, Clip> = {};
const ctx = (require as any).context("./events", false, /\.events\.json$/);
for (const k of ctx.keys()) {
  const c = ctx(k) as Clip;
  clips[c.scene] = c;
}

export type Cut = { kind: "scene"; spec: SceneSpec } | { kind: "title"; beats: number } | { kind: "outro"; beats: number };
export const FPS = 60;
const beatF = beats.beat_s * FPS;

/** Start frame of each cut: cut k ends on a beat line (offset + n * beat). */
export function layout(cuts: Cut[]) {
  let beatIdx = 0;
  const out: { cut: Cut; from: number; dur: number }[] = [];
  let from = 0;
  for (const cut of cuts) {
    const b = cut.kind === "scene" ? cut.spec.beats : cut.beats;
    beatIdx += b;
    const end = Math.round((beats.offset_s * FPS) + beatIdx * beatF);
    out.push({ cut, from, dur: end - from });
    from = end;
  }
  return { parts: out, total: from };
}

const Background: React.FC = () => (
  <AbsoluteFill style={{ background: `radial-gradient(120% 90% at 50% 0%, #232832 0%, ${T.bg0} 62%)` }}>
    <AbsoluteFill style={{ background: "radial-gradient(40% 30% at 85% 110%, rgba(210,117,86,.10), transparent 70%)" }} />
  </AbsoluteFill>
);

const Title: React.FC<{ dur: number }> = ({ dur }) => {
  const f = useCurrentFrame();
  const o = interpolate(f, [0, 14, dur - 10, dur], [0, 1, 1, 0], { extrapolateRight: "clamp" });
  const y = interpolate(f, [0, 22], [18, 0], { extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", opacity: o, transform: `translateY(${y}px)` }}>
      <Img src={staticFile("logo.png")} style={{ width: 132, height: 132, imageRendering: "pixelated", marginBottom: 26 }} />
      <div style={{ font: `700 120px ${T.font}`, color: T.text, letterSpacing: "-2px" }}>Kiln</div>
      <div style={{ font: `500 46px ${T.font}`, color: T.muted, marginTop: 10 }}>Every Claude Code and Codex session, one window</div>
    </AbsoluteFill>
  );
};

const Outro: React.FC<{ dur: number }> = ({ dur }) => {
  const f = useCurrentFrame();
  const o = interpolate(f, [0, 16], [0, 1], { extrapolateRight: "clamp" });
  const sc = interpolate(f, [0, 30], [0.94, 1], { extrapolateRight: "clamp" });
  const lineO = interpolate(f, [18, 34], [0, 1], { extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", opacity: o }}>
      <div style={{ display: "flex", alignItems: "center", gap: 36, transform: `scale(${sc})` }}>
        <Img src={staticFile("logo.png")} style={{ width: 150, height: 150, imageRendering: "pixelated" }} />
        <div style={{ font: `700 150px ${T.font}`, color: T.text, letterSpacing: "-3px" }}>Kiln</div>
      </div>
      <div style={{ font: `600 58px ${T.font}`, color: T.text, marginTop: 34, opacity: lineO }}>Never miss an agent waiting on you.</div>
      <div style={{ font: `500 40px ${T.font}`, color: T.clay, marginTop: 26, opacity: lineO }}>github.com/tkhangg0910/kiln</div>
      <div style={{ font: `400 34px ${T.font}`, color: T.muted, marginTop: 40, opacity: lineO * 0.9 }}>
        Windows 10/11 · MIT · forked from clayo-rust, a Rust port of Clayo
      </div>
    </AbsoluteFill>
  );
};

export const Video: React.FC<{ cuts: Cut[]; maxZoom: number; zoomScale: number; bgm: boolean; watermark?: boolean }> = ({
  cuts, maxZoom, zoomScale, bgm, watermark,
}) => {
  const { fps } = useVideoConfig();
  const { parts, total } = layout(cuts);
  const X = 8; // cross-dissolve frames
  return (
    <AbsoluteFill style={{ background: T.bg0 }}>
      <Background />
      {parts.map(({ cut, from, dur }, i) => (
        <Sequence key={i} from={Math.max(0, from - (i ? X : 0))} durationInFrames={dur + (i ? X : 0)}>
          <Fade inF={i ? X : 0} dur={dur + (i ? X : 0)}>
            {cut.kind === "scene" ? (
              <Scene spec={cut.spec} data={clips[cut.spec.clip]} maxZoom={maxZoom} zoomScale={zoomScale} durFrames={dur} />
            ) : cut.kind === "title" ? <Title dur={dur} /> : <Outro dur={dur} />}
          </Fade>
        </Sequence>
      ))}
      {/* Clicks: a soft click on every click event that is on screen. */}
      {parts.flatMap(({ cut, from, dur }, i) => {
        if (cut.kind !== "scene") return [];
        return clips[cut.spec.clip].events
          .filter((e) => e.type === "click")
          .map((e, k) => ({ f: from + Math.round((e.t_ms / 1000 - cut.spec.from) * fps) }))
          .filter(({ f }) => f >= from && f < from + dur - 3)
          .map(({ f }, k) => (
            <Sequence key={`c${i}-${k}`} from={f} durationInFrames={10}>
              <Audio src={staticFile("audio/click.wav")} volume={0.55} />
            </Sequence>
          ));
      })}
      {bgm && <Audio src={staticFile("audio/bgm.mp3")}
        volume={(f) => 0.42 * interpolate(f, [0, 20, total - 70, total], [0, 1, 1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })} />}
    </AbsoluteFill>
  );
};

const Fade: React.FC<{ inF: number; dur: number; children: React.ReactNode }> = ({ inF, dur, children }) => {
  const f = useCurrentFrame();
  const o = inF ? interpolate(f, [0, inF], [0, 1], { extrapolateRight: "clamp" }) : 1;
  return <AbsoluteFill style={{ opacity: o }}>{children}</AbsoluteFill>;
};

export const totalFrames = (cuts: Cut[]) => layout(cuts).total;
