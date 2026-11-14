# Configuration

Everything is set from **Settings** (gear button, bottom-left of the sidebar). Kilno saves it to
`%LOCALAPPDATA%\Kilno\settings.json`; you rarely need to edit that file by hand.

## Settings pages

| Page | What you set |
|---|---|
| General | Start at login, check for updates. Closing the window keeps sessions running; quit from the gear menu or the island. |
| Status bar | Theme (Numbers, Bars, Rings, Chips), which items show in the pane header (model, branch, context, effort, 5-hour and 7-day limits), the limits in the sidebar footer, and a warning near a limit (70, 80 or 90 %). |
| Security | "Ask before" rules: the Claude sessions Kilno starts stop and ask before they push, tag, release, merge or change the repo on GitHub. Your own Claude rules are shown read-only. |
| Agents | Reopens the setup window: use Claude Code, Codex or both, pick the default for New session, or point Kilno at an agent binary. |
| Island | Ctrl+Alt+S sends a screenshot to the active session; or have the island offer every new screenshot instead. |

## settings.json keys

| Key | Default | Meaning |
|---|---|---|
| `theme` | `numbers` | `numbers`, `bars`, `rings` or `chips` |
| `statusHeader`, `statusFooter` | `true` | Status in the pane header / sidebar footer |
| `statusModel`, `statusBranch`, `statusContext`, `statusEffort`, `statusFiveHour`, `statusSevenDay` | `true` | Individual header items |
| `reserveAt` | `0` | Warn at 70, 80 or 90 % of a limit; `0` is off |
| `useClaude`, `useCodex` | `true` | Which agents Kilno lists and starts |
| `defaultAgent` | `claude` | `claude`, `codex` or `ask` for New session |
| `claudePath`, `codexPath` | unset | Use this agent binary instead of the one on PATH |
| `screenshotHotkey` | `true` | Own Ctrl+Alt+S |
| `screenshotOffer` | `false` | Offer each new screenshot from the island |
| `updateCheck` | `true` | Check GitHub releases of `tkhangg0910/kilno` |

Start at login is not in this file: it is the `Kilno` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## Environment

| Variable | Effect |
|---|---|
| `CLAUDE_CONFIG_DIR` | Where Kilno reads Claude Code sessions (default `%USERPROFILE%\.claude`) |
| `CODEX_HOME` | Where Kilno reads Codex sessions (default `%USERPROFILE%\.codex`) |
| `KILNO_STARTUP_LOG=<file>` | Appends startup timing marks to that file |
