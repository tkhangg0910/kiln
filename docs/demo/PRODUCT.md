# Kiln — product brief (for the demo)

## What it does

Kiln puts every Claude Code and Codex session on a Windows PC into one native window. A sidebar
lists the sessions by project (resume, rename, search, branch), each pane is a real terminal running
the agent, and a small "island" drops from the top of the screen when a session needs you, hits an
error or finishes — even while you work in another app. One 18 MB `kiln.exe`, no WebView, no runtime.

## Who it is for

Developers on Windows 10/11 who run several coding-agent sessions (Claude Code and/or Codex) at once
and lose time switching terminals to find the one that is waiting for them.

## Five standout features

1. **All sessions in one window** — sidebar grouped by folder (Open / Today / Earlier), search,
   rename (F2), resume with a click, CC/CX tags when both agents are in use.
2. **The island** — needs-you / error / done / limit notices at the top of the screen; never takes
   focus; click it to jump to the session; drop a file or folder on it to start a session there.
3. **Branch from here** — fork a conversation into a new session, shown under its parent.
4. **Status at a glance** — model, git branch and diff, context and effort in each pane header;
   5-hour and 7-day limits in the sidebar footer (Numbers / Bars / Rings / Chips); a one-time
   reserve warning.
5. **Screenshot to a session** — Win+Shift+S, then Ctrl+Alt+S: the island asks which session, and the
   image lands in that prompt as an attachment.

Also: doc/page/image chips for what a session wrote or pasted, Settings, a first-run setup window,
start at login, `kiln` in Explorer's address bar, self-update with SHA-256 checks.

## Install and run

```powershell
irm https://raw.githubusercontent.com/tkhangg0910/kiln/main/scripts/install.ps1 | iex
```

installs the latest release to `%LOCALAPPDATA%\Programs\Kiln` and starts it; the setup window finds
Claude Code / Codex. From source: `cargo build --release` → `target\release\kiln.exe`.

## Product type and how the demo is recorded

- **Type:** Windows desktop app (Rust, eframe/egui + a Win32 layered window for the island).
- **Driving it:** pywinauto with the UIA backend. egui exposes its widgets through AccessKit, so
  buttons are found by their accessible names (their tooltip text). The terminal and the island have
  no inner UIA elements; steps on them use keys, or window-relative points with the reason noted.
- **Recording:** ffmpeg `gdigrab`, the 1920×1008 work area (taskbar excluded), 60 fps, x264 crf 16.
- **Isolation:** the debug build (`target\debug\kiln.exe`, its own `.Dev` single-instance lock) runs
  with `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `LOCALAPPDATA` inside `demo-data\env\`. Nothing of the
  user's real configuration, sessions or accounts is read or written.
- **Stand-in agent:** a real demo would need a signed-in Claude Code account. Instead each pane runs
  `demo-data\agent.py` (through the debug-build hook `KILN_DEV_AGENT_CMD`): it prints the kind of
  progress an agent does, asks "Do you want to…" when it needs approval, writes its transcript where
  Kiln reads sessions, and reports model / context / limits through Kiln's own status-line relay
  (`kiln.exe --statusline`). Everything Kiln shows around it — sidebar, header strip, limits, island,
  chips, picker — is Kiln's real behaviour. The stand-in has its own neutral look; it does not imitate
  Claude Code's interface.
