# Configuration

Everything is set from **Settings** (gear button, bottom-left of the sidebar). Kilno saves it to
`%LOCALAPPDATA%\Kilno\settings.json`; you rarely need to edit that file by hand.

## Settings pages

| Page | What you set |
|---|---|
| General | Start at login, a background image, check for updates. Closing the window keeps sessions running; the Kilno icon in the notification area (or the island) brings it back. Quit from the gear menu, the island or the icon's menu. |
| Status bar | Theme (Numbers, Bars, Rings, Chips), which items show in the pane header (model, branch, context, effort, 5-hour and 7-day limits, a model's own weekly limit such as Fable's), the limits in the sidebar footer, and a warning near a limit (70, 80 or 90 %). Live usage: ask Anthropic for the limits `/usage` shows (off by default). Click a limit in the footer to read it again. |
| Security | "Ask before" rules: the Claude sessions Kilno starts stop and ask before they push, tag, release, merge or change the repo on GitHub. Your own Claude rules are shown read-only. |
| Agents | Reopens the setup window: use Claude Code, Codex, OpenCode or any mix, pick the default for New session, or point Kilno at an agent binary. |
| Island | Ctrl+Alt+S sends a screenshot to the active session; or have the island offer every new screenshot instead. |

## settings.json keys

| Key | Default | Meaning |
|---|---|---|
| `theme` | `numbers` | `numbers`, `bars`, `rings` or `chips` |
| `statusHeader`, `statusFooter` | `true` | Status in the pane header / sidebar footer |
| `statusModel`, `statusBranch`, `statusContext`, `statusEffort`, `statusFiveHour`, `statusSevenDay` | `true` | Individual header items |
| `statusModelWeekly` | `true` | A model's own weekly limit (Fable, on Team premium seats and Max plans) with the account's; written only when `false` |
| `reserveAt` | `0` | Warn at 70, 80 or 90 % of a limit; `0` is off |
| `reserveFiveHour`, `reserveSevenDay`, `reserveModelWeekly` | `true` | Which limits that warning watches (`reserveModelWeekly` written only when `false`) |
| `liveUsage` | `false` | Ask Anthropic's usage endpoint for the limits with the Claude sign-in; written only when `true` |
| `liveUsageMinutes` | `2` | How often while the window shows: 1, 2 or 5; written only when not 2 |
| `useClaude`, `useCodex` | `true` | Which agents Kilno lists and starts |
| `useOpenCode` | `false` | OpenCode too; written only when `true` |
| `defaultAgent` | `claude` | `claude`, `codex`, `opencode` or `ask` for New session |
| `claudePath`, `codexPath`, `openCodePath` | unset | Use this agent binary instead of the one on PATH |
| `backgroundImage` | unset | A picture drawn faintly over the whole window (cropped to fill it); written only when set |
| `backgroundOpacity` | `15` | How strongly it shows, in percent: 5, 10, 15, 20 or 30; written only when not 15 |
| `screenshotHotkey` | `true` | Own Ctrl+Alt+S |
| `screenshotOffer` | `false` | Offer each new screenshot from the island |
| `updateCheck` | `true` | Check GitHub releases of `tkhangg0910/kilno` |

Start at login is not in this file: it is the `Kilno` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. It is off until you turn it on in setup or
Settings › General; Kilno never adds it by itself (Windows Defender treats an app that does so on
every start as malware, `Behavior:Win32/Persistence.A!ml`). If you move Kilno, the next start
points the value at the new place.

## Environment

| Variable | Effect |
|---|---|
| `CLAUDE_CONFIG_DIR` | Where Kilno reads Claude Code sessions (default `%USERPROFILE%\.claude`) and Claude Code's `.claude.json` (default `%USERPROFILE%\.claude.json`) |
| `CODEX_HOME` | Where Kilno reads Codex sessions (default `%USERPROFILE%\.codex`) |
| `XDG_DATA_HOME` | Where Kilno reads OpenCode's `opencode\opencode.db` (default `%USERPROFILE%\.local\share\opencode`) |
| `KILNO_STARTUP_LOG=<file>` | Appends startup timing marks to that file |
