# FAQ

**Do I need an account for Kilno?**
No. Kilno runs the Claude Code and Codex CLIs you already have and uses their sign-in.

**Does Kilno send my code or prompts anywhere?**
No. Its only network call is the GitHub releases check of `tkhangg0910/kilno`, which you can turn
off in Settings › General.

**Does it change my Claude Code or Codex settings?**
No. It reads `%USERPROFILE%\.claude` and `%USERPROFILE%\.codex` (or `CLAUDE_CONFIG_DIR` /
`CODEX_HOME`) and never writes there. Its own files live in `%LOCALAPPDATA%\Kilno`.

**What happens to running sessions when I close the window?**
They keep running and the island keeps watching them. Quit from the gear menu or the island to
end them.

**Can I run Kilno next to Clayo or clayo-rust?**
Yes. Each has its own data folder, single-instance lock, login entry and update channel. Only one
can own the Ctrl+Alt+S shortcut. To carry your Clayo settings over, copy `settings.json`,
`names.json` and `parents.json` from `%LOCALAPPDATA%\Clayo` to `%LOCALAPPDATA%\Kilno`.

**Which agent features work where?**
See [AGENTS.md](AGENTS.md) for what works with Claude Code and with Codex.

**macOS or Linux?**
No. Kilno is built on ConPTY and Win32 and runs on Windows 10 and 11 (64-bit) only.
