# Shot list

## Tagline

**Never miss an agent waiting on you.** (6 words)

## Three benefits

1. Every Claude Code and Codex session lives in one window, grouped by project.
2. The island taps you on the shoulder the moment a session needs you — then gets out of the way.
3. Model, context, branch and usage limits are visible without asking the agent.

## Promo video (30–45 s) — `docs/demo/promo.mp4`

| # | Scene (raw clip) | Duration | Action | Zoom on | On-screen text (≤ 6 words) |
|---|---|---|---|---|---|
| 1 | `01-problem` | 4 s | Four terminal windows tiled, each with an agent scrolling; one quietly asks a question nobody sees | the question line, 1.6× | Four agents. Four terminals. |
| 2 | `02-one-window` | 7 s | Kiln full screen: sidebar with today's sessions; click "Add rate limiting to orders" → its pane works, header strip fills in | the clicked row, then the header strip, 1.8× | Every session, one window |
| 3 | `03-island` | 7 s | Reading docs in another window; the island drops "Add rate limiting… · needs you"; click it → Kiln jumps to that session | the island, 2.0× | It tells you who's waiting |
| 4 | `04-answer` | 5 s | Press 1 to approve; the agent finishes; the dot turns green | the prompt, 1.6× | Answer and move on |
| 5 | `05-screenshot` | 7 s | Ctrl+Alt+S with a screenshot on the clipboard → island picker → press 1 → `[Image #1]` lands in the prompt | the picker, then the prompt, 1.8× | Screenshots straight to the prompt |
| 6 | `06-status` | 4 s | Hover the header strip and the 5h / 7d limits in the footer | strip, footer, 1.8× | Limits and context, always visible |
| 7 | (rendered) outro | 3 s | Logo, tagline, github.com/tkhangg0910/kiln | — | Kiln · never miss a waiting agent |

Total ≈ 37 s.

## Full video (60–120 s) — `docs/demo/demo-full.mp4`

Real end-to-end use, slower, zoom ≤ 1.6×:

1. `10-new-session` (10 s): New session → folder menu → `atlas-api` → the pane starts, the agent begins.
2. `02-one-window` (7 s): sidebar overview, open another session.
3. `11-search-resume` (9 s): type "dark" in Search → the "Dark mode for settings" session → open it.
4. `03-island` + `04-answer` (12 s): needs-you from another app, jump back, approve, done.
5. `12-branch` (8 s): Branch from here → the branch appears under its parent.
6. `13-doc-chip` (9 s): the agent wrote a doc → doc chip → doc viewer beside the terminal.
7. `05-screenshot` (7 s): screenshot to the prompt.
8. `14-settings` (10 s): Settings › Status bar → theme Rings → the strip and footer redraw.
9. `06-status` (4 s), outro (3 s).

Total ≈ 80 s.

## README screenshots — `docs/assets/`

| File | Content | Region |
|---|---|---|
| `sessions.png` | Main window: sidebar with Open/Today/Earlier, a pane working, header strip | Kiln window |
| `island.png` | The island showing "needs you" over another app | top-centre 1100×300 of the screen |
| `picker.png` | Screenshot picker listing sessions | top-centre 1100×420 of the screen |
| `status.png` | Header strip + footer limits in the Rings theme | Kiln window |
| `doc-viewer.png` | Doc chip open: Markdown beside the terminal | Kiln window |
| `settings.png` | Settings › Status bar | Kiln window |
