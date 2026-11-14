# Architecture

Kilno is a single native Windows process (`kilno.exe`) that hosts several terminal panes, each running a Claude Code or Codex session, plus a separate always-on-top "island" window that reports which session needs attention. No WebView, no embedded runtime.

Origin: Kilno is forked from [clayo-rust](https://github.com/tkhangg0910/clayo-rust), the Rust port of [Clayo](https://github.com/YogiOnCode/Clayo) (C#/WPF, MIT) by Yogeswaran Amsavalli.

## Layers

```
kilno-app   (eframe/egui UI, setup + settings windows, glue)
 ├─ kilno-term    VT parser + egui terminal widget
 ├─ kilno-pty     ConPTY child processes
 ├─ kilno-island  layered Win32 island window (own thread)
 └─ kilno-core    everything without a UI (pure logic, file formats, state machines)
```

`kilno-core` has no UI dependency, so most behaviour is testable with temp folders. `kilno-app` is the only crate that depends on all the others.

## Components

| Component | Responsibility |
|---|---|
| `kilno-core::sessions` | Reads Claude Code (`~/.claude/projects`) and Codex (`~/.codex/sessions`) transcripts: model, tail, change detection, `notify`-based watching. |
| `kilno-core::status` | Status-line relay, per-session status store (`<data>\status\<guid>.json`), strip formatting, reserve/limit warnings. |
| `kilno-core::island` | The island's state machine (`trigger`) and the drop-to-ask mention text. |
| `kilno-core::agent` | Detection and setup of the Claude and Codex executables. |
| `kilno-core::settings`, `store` | `settings.json`, `names.json`, `parents.json` in the data folder. |
| `kilno-core::security` | "Ask before" rules Kilno adds to sessions it starts; the user's own rules are read-only. |
| `kilno-core::pages` | Agent-written HTML pages (`<project>\.kilno\pages\*.html`), sealed with a Content-Security-Policy. |
| `kilno-core::system` | Login-start (Run key), user PATH, GitHub releases updater. |
| `kilno-pty` | Spawns a child under a pseudoconsole; a read thread delivers raw bytes; teardown kills the child, then closes the console off-thread. All `unsafe` lives in `sys.rs`. |
| `kilno-term` | `alacritty_terminal` grid, colours, key/mouse encoding, egui widget. Bytes are parsed on the PTY read thread; the UI only locks the grid to draw. Scrollback is 5000 lines. |
| `kilno-island` | Layered, never-focused Win32 window on its own STA thread, painted with `tiny-skia`; glow, mascot, picker, drop target, screenshots. |
| `kilno-app` | Main window (sidebar, splitter, header, panes), setup window, Settings, island host, single-instance handling, updater, doc/page viewers. |

## Data flow

1. **Pane output**: child process -> ConPTY -> read thread (`kilno-pty`) -> `Terminal::feed` (`kilno-term`) -> egui draws a frame.
2. **Pane input**: egui events -> `kilno-term::input` encoding -> PTY write.
3. **Sessions**: a sessions thread watches the agents' folders (read-only) and feeds the sidebar model. Pane events are handled in `App::logic`, which keeps running while the window is hidden (closing the window only hides it).
4. **Status**: Claude panes start with a status line that runs `kilno.exe --statusline [--pass]`. Claude Code pipes session JSON to it on every refresh; Kilno stores a copy and, with `--pass`, forwards the same bytes to the user's own status line command. `main()` handles this argument before any window, font or mutex exists, because it runs on every refresh.
5. **Island**: `kilno-core::island::trigger` decides when to show; `kilno-island` draws; requests that touch panes go back to the UI as `AppEvent::Island` via `island_host`.

## Process model

- One Kilno per user session: a named mutex. A second launch sends its folder over a named pipe to the first and exits.
- The process ends with `std::process::exit`, so no stray thread (PTY reader, watcher) keeps the mutex alive.
- `panic = "abort"` in release; a panic hook first appends to `%LOCALAPPDATA%\Kilno\crash.log`.
- Children start as top-level sessions: inherited Claude session variables are scrubbed.

## Read-only guarantee

Paths under the agents' folders are `ReadOnlyDir` values: joinable and readable, never writable. `crates/kilno-core/tests/invariants.rs` guards this. Kilno writes only to `%LOCALAPPDATA%\Kilno`, the `Run` registry value, and the user `Path`.

## Rendering and safety

eframe with the OpenGL (glow) backend and a Direct3D 12 wgpu fallback (WARP when there is no GPU). `unsafe_code` is denied in `kilno-core` and `kilno-term` (workspace lint). The crates that need Win32 (`kilno-app`, `kilno-pty`, `kilno-island`) deny `unsafe_op_in_unsafe_fn` and require a `// SAFETY` comment on every unsafe block (`clippy::undocumented_unsafe_blocks`).
