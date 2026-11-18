# FAQ

**Do I need an account for Kilno?**
No. Kilno runs the Claude Code, Codex and OpenCode CLIs you already have and uses their sign-in.

**Does Kilno send my code or prompts anywhere?**
No. Its only network calls are the GitHub releases check of `tkhangg0910/kilno` (Settings ›
General) and, if you turn on live usage (Settings › Status bar, off by default), Anthropic's usage
endpoint, asked with your Claude sign-in for the limits `/usage` shows.

**Does it change my Claude Code, Codex or OpenCode settings?**
No. It reads `%USERPROFILE%\.claude` and `%USERPROFILE%\.codex` (or `CLAUDE_CONFIG_DIR` /
`CODEX_HOME`) and never writes there. Its own files live in `%LOCALAPPDATA%\Kilno`.

**Where does the Fable limit come from?**
Claude Code does not hand a model's own weekly limit to its status line, but it saves the last
usage reply it fetched in `%USERPROFILE%\.claude.json` (`cachedUsageUtilization`). Kilno reads
that reply and the signed-in account's id from the file, nothing else: your email, MCP server
settings and project list are skipped while parsing and never kept. No sign-in token is read and
no request is made, unless you turn on live usage (below). A reply saved for another account is
not shown. The key is Claude Code's own and undocumented; if it changes, the extra limit simply
stops showing.

**Why is a limit a few minutes behind?**
By default Kilno makes no request of its own: the 5-hour and 7-day limits come with each Claude
reply's status line, which can trail `/usage` by a point for a while, and the Fable limit from the
usage reply Claude Code saves now and then. Kilno picks either up the moment it is written; a
limit's tooltip says how old it is. Past its reset time a limit shows 0 % until the next report.

For the numbers `/usage` shows, turn on **Live usage** in Settings › Status bar. Kilno then asks
Anthropic's usage endpoint itself: every 1, 2 or 5 minutes while the window shows, after a Claude
reply (at most once a minute) and when you click a limit. It reads only the access token from
Claude Code's `.credentials.json` (not the refresh token), sends it to `api.anthropic.com` alone
(redirects are not followed), and never saves or logs it. An expired token is not sent; Claude
Code renews it.

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
See [AGENTS.md](AGENTS.md) for what works with Claude Code, Codex and OpenCode.

**macOS or Linux?**
Linux x86_64: yes, as a preview; see [linux.md](linux.md). macOS: no.
