# Technical decisions

Origins: Clayo (Yogeswaran Amsavalli, C#/WPF, MIT) -> clayo-rust (native Rust port) -> Kiln. `PORTING.md` holds the port's design notes.

| Decision | Why | Trade-off |
|---|---|---|
| Native Rust + egui instead of C#/WPF/WebView | One small `kiln.exe`, no runtime; footprint measured with `bench/` | Terminal widget, island and viewers rebuilt by hand |
| `alacritty_terminal` for VT parsing | Mature parser; honours DEC 2026 synchronized updates used by Claude Code's full-screen redraws | Tied to alacritty's API |
| PTY bytes parsed on the read thread | A flood (`cat` of 10 MB) never blocks the UI; the UI only locks the grid to draw | Grid shared behind a lock |
| Raw bytes from ConPTY, never decoded in `kiln-pty` | A read can split UTF-8 or escape sequences | Decoding is the parser's job |
| Scrollback 5000 lines | 20000 lines cost about 29 MB per pane | Shorter history |
| Island is its own Win32 window on its own thread, painted with tiny-skia | Must never take focus and must stay alive while the main window is hidden | Separate window plumbing; talks to the app through events |
| Logic in `kiln-core` | Testable without a UI; the read-only rule lives in one place | More crates and indirection |
| `ReadOnlyDir` type for agent folders | The compiler stops writes under `.claude` / `.codex` | Wrapper types |
| Status via `kiln.exe --statusline --pass` | Gets Claude Code's per-refresh JSON without editing the user's settings; forwards to their own status line | `main()` must handle it first and stay cheap |
| `settings.json` written byte-compatible with the C# build, read key by key | Switching builds does not churn the file; one bad value costs one setting | Hand-written serializer |
| Agent pages sealed with a strict CSP | A page an agent wrote cannot send data anywhere | Only listed assets/CDNs work |
| `unsafe` confined to FFI, `// SAFETY` required | Auditability | Boilerplate |
| Release profile: fat LTO, `panic = "abort"`, strip; panic hook writes `crash.log` | Small, fast binary that still records why it died | No unwinding |
| Single instance via mutex + named pipe | A second launch hands over its folder | |
| Own data folder, mutex, Run entry and update channel | Installs beside Clayo and clayo-rust | Only one app owns Ctrl+Alt+S |
| Tag-on-main releases with provenance attestation; actions pinned to commit SHAs | Verifiable downloads; tags can be moved, SHAs cannot | Manual pin updates |
| `bench/` outside the workspace | Never ships in `kiln.exe` | Separate lockfile |
