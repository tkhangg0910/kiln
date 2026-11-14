# Troubleshooting

**Ctrl+Alt+S does nothing / Settings › Island says "taken".**
Another app (Clayo, a screenshot tool, an IME) owns the shortcut. Close it, or turn on
"Offer to send every new screenshot": take screenshots with any tool and send them from the
island.

**New session says the agent was not found.**
Open Settings › Agents. The setup window finds Claude Code and Codex on PATH, can install them,
or lets you pick the binary (saved as `claudePath` / `codexPath`).

**A session is missing from the sidebar.**
Kilno lists what is in `%USERPROFILE%\.claude` and `%USERPROFILE%\.codex`. If you moved them, set
`CLAUDE_CONFIG_DIR` / `CODEX_HOME` before starting Kilno. Check that the agent is switched on in
Settings › Agents.

**No branch or diff in the pane header.**
That needs `git` on PATH and a folder inside a git repository.

**The window does not open; only the island shows.**
Kilno was started with `--background` (start at login). Run `%LOCALAPPDATA%\Programs\Kilno\kilno.exe`
again to show the window.

**Claude Code has no colours in a pane.**
Something in Kilno's environment sets `NO_COLOR`. Claude Code sets it for the commands it runs,
so a Kilno started from inside a Claude Code session used to pass it on; since 0.2.0 Kilno drops
it in that case. A `NO_COLOR` you set yourself in Windows is kept: remove it there.

**A new session is slow to start, or scrolls jerkily for a few seconds.**
Check that you run a release build (the installed `kilno.exe` or `target\release\kilno.exe`),
not `target\debug\kilno.exe`: the debug build is unoptimised and draws the terminal many times
slower, which shows while Claude Code paints its first screens. With a release build a new
Claude session shows its first screen in about as long as `claude` takes in a plain console
(2–3 s); most of that is Claude Code starting.

**Kilno crashed or starts slowly.**
Crash details go to `%LOCALAPPDATA%\Kilno\crash.log`. For start-up timing, run it once with
`KILNO_STARTUP_LOG=<file>` set. Attach both when you
[open an issue](https://github.com/tkhangg0910/kilno/issues).
