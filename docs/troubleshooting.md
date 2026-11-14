# Troubleshooting

**Ctrl+Alt+S does nothing / Settings › Island says "taken".**
Another app (Clayo, a screenshot tool, an IME) owns the shortcut. Close it, or turn on
"Offer to send every new screenshot": take screenshots with any tool and send them from the
island.

**New session says the agent was not found.**
Open Settings › Agents. The setup window finds Claude Code and Codex on PATH, can install them,
or lets you pick the binary (saved as `claudePath` / `codexPath`).

**A session is missing from the sidebar.**
Kiln lists what is in `%USERPROFILE%\.claude` and `%USERPROFILE%\.codex`. If you moved them, set
`CLAUDE_CONFIG_DIR` / `CODEX_HOME` before starting Kiln. Check that the agent is switched on in
Settings › Agents.

**No branch or diff in the pane header.**
That needs `git` on PATH and a folder inside a git repository.

**The window does not open; only the island shows.**
Kiln was started with `--background` (start at login). Run `%LOCALAPPDATA%\Programs\Kiln\kiln.exe`
again to show the window.

**Kiln crashed or starts slowly.**
Crash details go to `%LOCALAPPDATA%\Kiln\crash.log`. For start-up timing, run it once with
`KILN_STARTUP_LOG=<file>` set. Attach both when you
[open an issue](https://github.com/tkhangg0910/kiln/issues).
