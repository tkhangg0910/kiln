# FAQ

**Do I need an account for Kilno?**
No. Kilno runs the Claude Code and Codex CLIs you already have and uses their sign-in.

**Does Kilno send my code or prompts anywhere?**
No. Its only network call is the GitHub releases check of `tkhangg0910/kilno`, which you can turn
off in Settings › General.

**Does it change my Claude Code or Codex settings?**
No. It reads `%USERPROFILE%\.claude` and `%USERPROFILE%\.codex` (or `CLAUDE_CONFIG_DIR` /
`CODEX_HOME`) and never writes there. Its own files live in `%LOCALAPPDATA%\Kilno`.

**Where does the Fable limit come from?**
Claude Code does not hand a model's own weekly limit to its status line, but it saves the last
usage reply it fetched in `%USERPROFILE%\.claude.json` (`cachedUsageUtilization`). Kilno reads
that reply and the signed-in account's id from the file, nothing else: your email, MCP server
settings and project list are skipped while parsing and never kept. No sign-in token is read and
no request is made. A reply saved for another account is not shown. The key is Claude Code's own
and undocumented; if it changes, the extra limit simply stops showing.

**What happens to running sessions when I close the window?**
They keep running and the island keeps watching them. The Kilno icon in the notification area
shows the window again on a click, and its menu has Show Kilno and Quit Kilno. Windows may put a
new icon under the ^ overflow; drag it onto the taskbar to keep it in view. Quit from that menu,
the gear menu or the island to end the sessions.

**Can I run Kilno next to Clayo or clayo-rust?**
Yes. Each has its own data folder, single-instance lock, login entry and update channel. Only one
can own the Ctrl+Alt+S shortcut. To carry your Clayo settings over, copy `settings.json`,
`names.json` and `parents.json` from `%LOCALAPPDATA%\Clayo` to `%LOCALAPPDATA%\Kilno`.

**Which agent features work where?**
See [AGENTS.md](AGENTS.md) for what works with Claude Code and with Codex.

**macOS or Linux?**
No. Kilno is built on ConPTY and Win32 and runs on Windows 10 and 11 (64-bit) only.
