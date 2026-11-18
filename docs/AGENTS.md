# Claude Code, Codex and OpenCode in Kilno

Kilno runs your coding agent in its panes: Claude Code, Codex, OpenCode, or any mix. You need
at least one. The first time Kilno starts, its setup window finds them, installs or signs in to
one if you ask, and lets you choose which to use. **Settings › Agents** opens it again.

Checked with Claude Code 2.1.288, codex-cli 0.160.0 and OpenCode 1.19.6 on Windows 11.

## What works with each

| | Claude Code | Codex | OpenCode |
|---|---|---|---|
| Found and set up by the setup window | Yes | Yes (Install needs Node.js) | Yes (Install needs Node.js) |
| New session | Yes | Yes | Yes |
| Resume a session | Yes | Yes | Yes |
| Branch from here | Yes | Yes | Yes (`--fork`) |
| Sessions in the sidebar, by project | Yes | Yes | Yes |
| Session titles | Claude's own | Codex's own | OpenCode's own |
| Branches shown under the session they came from | Branches made in Kilno | Every fork, also ones made outside Kilno | Every fork, also ones made outside Kilno |
| Status light: working, done | Yes | Yes | Yes |
| Status light: needs you | Yes | Yes | Not yet: OpenCode's permission prompts are not recognised |
| Pane header: model, branch, context, effort | Yes | Yes (effort only when you set one) | Model and branch (OpenCode stores no context window size) |
| Usage limits | In the sidebar footer | In the pane header, for 5-hour and 7-day limits | None (OpenCode reports none) |
| Island notices (needs you, done) | Yes | Yes | Done only |
| Drop a file on the island | Yes | Yes | Yes |

With more than one in use, each sidebar row shows its agent's mark (Claude's, OpenAI's for
Codex, a square with a cursor for OpenCode), and **New session** starts the one you picked in
setup (or asks each time).

### Where they differ

- **A new Codex or OpenCode session shows in the sidebar after your first message.** Both pick
  their own session id, so Kilno finds the session once it appears. Claude takes the id Kilno
  gives it, so its row is there at once.
- **Codex limits.** Codex reports its limits per plan. A 5-hour or 7-day limit shows in the
  Codex pane's header; a plan whose only limit has another length (a monthly one, for
  example) shows none.
- **A Codex branch needs the session it came from.** Codex doesn't copy the history into
  the branch, it points back at the original. Deleting the original's file by hand breaks
  its branches.
- **OpenCode keeps its sessions in a database,** `opencode.db`, not in transcript files. Kilno
  reads it (read only) for the rows, titles and whether a turn is running. Archived sessions
  are not listed. OpenCode runs its free models without signing in, so its row is ready as
  soon as it is installed.

## Installing by hand

Use this if the setup window can't install an agent for you. Run the commands in
PowerShell, then press **Check again** in the setup window.

### Claude Code

```powershell
irm https://claude.ai/install.ps1 | iex   # installs %USERPROFILE%\.local\bin\claude.exe
claude auth login                          # sign in once
```

Check it with `claude --version` and `claude auth status`.

### Codex

Codex installs through npm, so it needs [Node.js](https://nodejs.org) first.

```powershell
npm install -g @openai/codex   # installs %APPDATA%\npm\codex.cmd
codex login                    # sign in once
```

Check it with `codex --version` and `codex login status`.

### OpenCode

OpenCode installs through npm too.

```powershell
npm install -g opencode-ai   # installs %APPDATA%\npm\opencode.cmd
opencode auth login          # optional: a provider of your own
```

Check it with `opencode --version`.

### Installed somewhere else?

Press **Locate…** on the agent's row in the setup window and pick its `claude.exe`,
`codex.cmd` or `opencode.cmd`. Otherwise Kilno looks on your PATH, then in the folders the
installers above use (and `%LOCALAPPDATA%\Microsoft\WinGet\Links`).

## What Kilno reads

- Claude Code sessions: `%USERPROFILE%\.claude\projects`
- Claude Code's last usage reply, for the limits (a model's own weekly limit such as Fable's is
  only there): `cachedUsageUtilization` and the account id in `%USERPROFILE%\.claude.json`, or
  in `%CLAUDE_CONFIG_DIR%` if you set it. The rest of that file is skipped.
- Codex sessions: `%CODEX_HOME%\sessions` if you set `CODEX_HOME`, else
  `%USERPROFILE%\.codex\sessions`
- OpenCode sessions: the `session`, `message` and `part` tables of
  `%USERPROFILE%\.local\share\opencode\opencode.db` (or `%XDG_DATA_HOME%\opencode`), opened
  read only

Kilno only reads these. It never opens the agents' sign-in files to see whether you're signed
in: it asks the agent (`claude auth status`, `codex login status`). The one exception is live
usage (Settings › Status bar, off by default): then it reads the access token in Claude Code's
`.credentials.json` to ask Anthropic's usage endpoint for your limits, and sends it nowhere else.
