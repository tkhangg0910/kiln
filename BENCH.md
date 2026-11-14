# BENCH.md — Clayo Rust vs Clayo C# 1.0.6

> **Kilno note.** Written for clayo-rust (the Rust port of Clayo) before the fork; there `clayo-*`
> crates, `clayo.exe` and `%LOCALAPPDATA%\Clayo` are Kilno's `kilno-*`, `kilno.exe` and `%LOCALAPPDATA%\Kilno`.

> **Layout note (clayo-rust).** This document was written in the original repository, where the Rust
> workspace lived in `rust/` next to the C# app in `src/Clayo`. In clayo-rust the workspace is at the
> repository root (`rust/crates/...` is now `crates/...`), the page assets are in `assets/pages`, and
> the C# source and `checks/` are in the original project: https://github.com/YogiOnCode/Clayo

Measured on 2026-11-13/14 on the same Windows 11 x64 laptop (150 % display scale), release builds:
the Rust port from branch `rust-port` (`rust/target/release/clayo.exe`) and the C# 1.0.6 release
(`%LOCALAPPDATA%\Programs\Clayo\clayo.exe`, installed by `scripts/install.ps1`).

Every run used throwaway folders: `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `LOCALAPPDATA` pointed at
temp folders (settings: `setupDone`, Codex off, update check off) and a temp work folder, so the
user's real configuration was never read or written. A pane runs the real `claude` 2.1.323 unless
noted (signed out, so it sits on its first-run screen).

## Result against the budget (RUST_PORT_PLAN.md)

| Metric | Budget | Rust | C# 1.0.6 | Pass |
|---|---|---|---|---|
| Cold start → window shown | < 200 ms | first frame painted **145–176 ms** after `main` (window visible 25–31 ms after launch, first run after a build 119 ms) | window visible **963–1425 ms**, first run 8 767 ms | ✅ |
| RAM idle (private working set, clayo only) | < 40 MB | **32.8 MB** (1 Claude pane), 27.0 MB (no pane) | 79.9 MB + 6 WebView2 helpers 143.5 MB | ✅ |
| RAM with 5 panes | < 120 MB | **47.6 MB** (5 Claude panes); **57.4 MB** with 5 panes each holding a full 5 000-line scrollback | 64.6 MB + 10 WebView2 helpers 345.0 MB (5 Claude panes) | ✅ |
| CPU idle | ~0 %, no busy timer, repaint only on output/input | **0.16 %** of one core with no pane, 0.39 % bare shell pane, 0.55 % Claude pane (see below); **0 frames drawn** once a pane has been quiet 5 s | 1.09 % in clayo.exe alone (rendering runs in the WebView2 helpers, not counted) | ✅ |
| Key → display latency | < 5 ms | Clayo's share **1.79 ms median, 2.43 ms p95, 3.01 ms max**; with the shell + ConPTY echo round trip (0.95 ms median) ≈ 2.7 ms | not measured (xterm.js in WebView2) | ✅ |
| `cat` of a 10 MB file | UI does not hang | 383 UI probes over 25 s, **worst reply 92.6 ms, 0 timeouts** | — | ✅ |
| `clayo.exe` size | < 20 MB, one file, no runtime | **18.2 MB** single exe (page assets for D3 embedded; 10.9 MB without them) | 134.6 MB folder, 260 files (self-contained .NET + WebView2 assets) | ✅ |

## How each number was taken

- **Tool**: `rust/bench` (`clayo-bench.exe`, not part of clayo.exe). It starts the exe with
  `CreateProcessW`, polls `EnumWindows` every 1 ms for a visible top-level window of that process,
  waits, reads `PROCESS_MEMORY_COUNTERS_EX2.PrivateWorkingSetSize`, and takes `GetProcessTimes`
  over an idle window; helper processes (WebView2) are found through the process tree.
  `clayo-bench <exe> --cwd <work> --runs 5 --settle 10 --idle 20 [--helpers msedgewebview2.exe]`
- **First frame**: `CLAYO_STARTUP_LOG=<file>` makes the Rust build write timing marks
  (`main`, `app new`, `fonts`, `pane started`, `first frame`). Three cold starts: 175.6 / 144.7 /
  147.7 ms. eframe shows the window before its first frame, so "window visible" alone would flatter
  the Rust build; the budget is checked against the first painted frame.
- **5 panes**: the first instance opens the work folder, four more `clayo <folder>` launches hand
  the folder to it over the single-instance pipe, each starting a new session; measured 30 s after
  the last one. The scrollback run used `CLAYO_BENCH_AGENT_CMD="cmd /c type big10mb.txt"` so each
  pane printed 10 MB first.
- **Idle CPU**: `CLAYO_FRAME_LOG=<file>` logs every frame with egui's repaint cause. With a quiet
  pane the log shows the cursor blink for 5 s (D14) and then no frame at all for the rest of the
  30 s run (only the 30 s clock tick for ages and countdowns remains). What is left is background
  threads: mainly the island's cursor poll (50 ms near the top edge, 250 ms elsewhere, as the C#
  island did — kept by D13), plus the ConPTY reader and the agent's own occasional output.
- **Key latency**: `CLAYO_BENCH_TYPE=<log>` makes the app type one character every 200 ms into a
  bare shell pane through the same path as a real key (`PaneShared::type_keys`) and log, for 200
  samples, "echo" (key written to the PTY → echo bytes read back: shell + ConPTY, not Clayo) and
  "frame" (echo read → end of the UI pass that draws it; vsync is off, so the GL present follows
  at once). PORTING.md §4.6 defines the metric this way.
- **10 MB `type`**: one pane runs `cmd /c type big10mb.txt` (10 MiB of mixed ASCII, Vietnamese and
  SGR colour); meanwhile `SendMessageTimeoutW(WM_NULL, 1000 ms)` probes the window's UI thread
  every 50 ms. Parsing happens on the pane's PTY read thread, the UI thread only locks the grid
  to draw, so the UI stays responsive.

## Notes

- The C# first start (8.8 s) includes WebView2 creating its user-data folder; later starts are
  ~1 s. The Rust first start after a build (119 ms) is Windows Defender scanning the new exe.
- Earlier runs with the screen locked gave C# 39 % idle CPU; those numbers were discarded and
  re-measured with the screen on.
- Scripts used (in `rust/bench/scripts`, set `CLAYO_BENCH_DIR` or use `%TEMP%\clayo-bench`; `make-big10mb.py` writes the 10 MB file): `bench-load.ps1` (10 MB flood + 5-pane scrollback) and `bench-5claude.ps1`
  (5 Claude panes, Rust vs C#) — both drive the release exe exactly as above.
