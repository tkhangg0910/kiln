# Usage

## Command line

```powershell
kilno.exe [folder] [--background]
```

- `folder`: the first argument that is an existing folder opens a session there. With none, Kilno uses the working directory, unless that is Kilno's own folder or System32 (shortcut/Start menu launches); then it opens on the home folder with no session.
- `--background`: used by start at login: no window, only the island, until asked for.
- Running `kilno.exe <folder>` while Kilno is already running hands the folder to the running instance and exits.
- `kilno.exe --statusline [--pass]` is internal (the status line relay), not meant to be run by hand.

## Main flows

1. First launch: the setup window finds Claude Code and/or Codex, can install or sign in, and lets you choose the default. Reopen it with Settings > Agents.
2. The sidebar lists sessions by project for both agents (each row marked with its agent's logo when both are used). Start a new session, resume one, or branch from one.
3. Each pane header shows model, branch, context and effort; a status light shows working, done or needs you.
4. The island at the top of the screen announces when a session needs you or is done, offers a picker, and accepts dropped files.
5. Closing the window only hides it; quit from the gear menu.
6. Ctrl+Alt+S is the screenshot shortcut. Only one of Kilno or Clayo can own it; the other shows it as taken in Settings > Island.

Per-agent differences and manual agent installs: [AGENTS.md](AGENTS.md).

## Pages

Agents can write HTML pages to `<project>\.kilno\pages\*.html`. Kilno seals them with a Content-Security-Policy (no connections, no form posts) before showing them, and keeps that folder out of git.

## Data and diagnostics

- Data: `%LOCALAPPDATA%\Kilno` (`settings.json`, `names.json`, `parents.json`, `status\`, `crash.log`).
- Moving from Clayo: copy `%LOCALAPPDATA%\Clayo\settings.json`, `names.json` and `parents.json` into `%LOCALAPPDATA%\Kilno`.
- `KILNO_STARTUP_LOG=<file>` appends startup timing marks to that file.
- Network: only the GitHub releases check of `tkhangg0910/kilno`.
