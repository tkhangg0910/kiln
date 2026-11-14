import React from "react";
import { Composition } from "remotion";
import { Cut, FPS, Video, totalFrames } from "./Video";

// Promo (30–45 s): problem → one window → island → answer → screenshot → status → outro.
export const PROMO: Cut[] = [
  { kind: "scene", spec: { clip: "01-problem", from: 0.0, beats: 8, caption: "Four agents. Four terminals.", captionAt: 0.2 } },
  { kind: "scene", spec: { clip: "02-one-window", from: 0.9, beats: 13, caption: "Every session, one window", captionAt: 0.15 } },
  { kind: "scene", spec: { clip: "03-island", from: 2.2, beats: 9, caption: "It tells you who's waiting", captionAt: 0.15 } },
  { kind: "scene", spec: { clip: "04-answer", from: 0.3, beats: 11, caption: "Answer and move on" } },
  { kind: "scene", spec: { clip: "05-screenshot", from: 0.3, beats: 15, caption: "Screenshots straight to the prompt" } },
  { kind: "scene", spec: { clip: "06-status", from: 0.2, beats: 10, caption: "Limits and context, always visible" } },
  { kind: "outro", beats: 7 },
];

// Full demo (60–120 s): real use from a new session to settings, slower, gentler zoom.
export const FULL: Cut[] = [
  { kind: "title", beats: 5 },
  { kind: "scene", spec: { clip: "10-new-session", from: 0.0, beats: 24, caption: "Start a session in any project" } },
  { kind: "scene", spec: { clip: "02-one-window", from: 0.9, beats: 13, caption: "Every session, one window", captionAt: 0.15 } },
  { kind: "scene", spec: { clip: "11-search-resume", from: 0.2, beats: 16, caption: "Find and resume any session" } },
  { kind: "scene", spec: { clip: "03-island", from: 2.2, beats: 9, caption: "It tells you who's waiting", captionAt: 0.15 } },
  { kind: "scene", spec: { clip: "04-answer", from: 0.3, beats: 11, caption: "Answer and move on" } },
  { kind: "scene", spec: { clip: "12-branch", from: 0.2, beats: 13, caption: "Branch it, keep the original" } },
  { kind: "scene", spec: { clip: "13-doc-chip", from: 0.1, beats: 10, caption: "Read what the agent wrote" } },
  { kind: "scene", spec: { clip: "05-screenshot", from: 0.3, beats: 15, caption: "Screenshots straight to the prompt" } },
  { kind: "scene", spec: { clip: "14-settings", from: 0.2, beats: 23, caption: "Pick how the numbers look" } },
  { kind: "scene", spec: { clip: "06-status", from: 0.2, beats: 10, caption: "Limits and context, always visible" } },
  { kind: "outro", beats: 7 },
];

export const Root: React.FC = () => (
  <>
    <Composition id="Promo" component={Video} fps={FPS} width={1920} height={1080} durationInFrames={totalFrames(PROMO)}
      defaultProps={{ cuts: PROMO, maxZoom: 2.0, zoomScale: 1, bgm: true }} />
    <Composition id="Full" component={Video} fps={FPS} width={1920} height={1080} durationInFrames={totalFrames(FULL)}
      defaultProps={{ cuts: FULL, maxZoom: 1.6, zoomScale: 0.75, bgm: true }} />
  </>
);
