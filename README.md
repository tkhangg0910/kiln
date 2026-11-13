# Kiln

Every Claude Code and Codex session in one native Windows window, plus an island at the top of the
screen that tells you which one needs you. One small `kiln.exe`, no WebView, no runtime.

Kiln is the personal line of development of [tkhangg0910](https://github.com/tkhangg0910), forked
from [clayo-rust](https://github.com/tkhangg0910/clayo-rust).

## Credits

- **[Clayo](https://github.com/YogiOnCode/Clayo)** by **Yogeswaran Amsavalli** is the original app
  (C#/WPF, MIT). Its design, behaviour, texts, mascot, icon and island are his work; Kiln starts
  from them.
- **[clayo-rust](https://github.com/tkhangg0910/clayo-rust)** is the native Rust port of Clayo
  (MIT) that Kiln is forked from. Its full git history, including every original Clayo commit,
  is kept in this repository.
- Kiln is not affiliated with or endorsed by the Clayo project, Anthropic or OpenAI.

The mascot and icon are still Clayo's (used under its MIT licence, with credit); Kiln will get its
own.

## Requirements

- Windows 10 or 11, 64-bit.
- [Claude Code](https://docs.claude.com/en/docs/claude-code) and/or [Codex](https://github.com/openai/codex).
- To build: Rust stable (1.85+) with the MSVC toolchain and the Visual Studio C++ build tools.
  `git` on PATH is used at run time for the branch/diff in the pane header.

## Build

```powershell
cargo build --release          # target\release\kiln.exe
cargo test
cargo clippy --all-targets -- -D warnings
```

| Crate | What it is |
|---|---|
| `kiln-core` | All logic with no UI: transcripts, sessions, status relay and store, settings, rules, island state machine, setup texts, login start, PATH, updater. |
| `kiln-pty` | ConPTY: spawn, read thread, write, resize, non-blocking teardown. |
| `kiln-term` | VT parsing (`alacritty_terminal`), colours, key/mouse encoding, the egui terminal widget. |
| `kiln-island` | The island: a layered Win32 window that never takes focus, glow, mascot, picker, drop target. |
| `kiln-app` | `kiln.exe`: eframe/egui (OpenGL, Direct3D 12 fallback) main window, setup window, Settings. |
| `bench` | `kiln-bench` and the scripts behind BENCH.md (not part of kiln.exe). |

## Install

```powershell
irm https://raw.githubusercontent.com/tkhangg0910/kiln/main/scripts/install.ps1 | iex
```

(once a release is published), or build the zip yourself with `.\scripts\publish.ps1` and unzip it
into `%LOCALAPPDATA%\Programs` (giving `%LOCALAPPDATA%\Programs\Kiln\kiln.exe`). Releases are built
by GitHub Actions from a `v*` tag on `main`, with a build-provenance attestation:
`gh attestation verify kiln-<version>-win-x64.zip --repo tkhangg0910/kiln`.

## Living next to Clayo

Kiln has its own data folder (`%LOCALAPPDATA%\Kiln`), single-instance lock, login-start entry and
update channel, so it can be installed beside Clayo or clayo-rust. Both read the same Claude Code
and Codex sessions (read only). Only one app at a time can own the Ctrl+Alt+S screenshot shortcut;
the other reports it as taken in Settings › Island. To move your Clayo settings over, copy
`%LOCALAPPDATA%\Clayo\settings.json`, `names.json` and `parents.json` into `%LOCALAPPDATA%\Kiln`.

## What Kiln touches

- Reads only: `%USERPROFILE%\.claude` (or `%CLAUDE_CONFIG_DIR%`) and `%USERPROFILE%\.codex` (or
  `%CODEX_HOME%`); never writes there.
- Writes its own files in `%LOCALAPPDATA%\Kiln`; registry: `HKCU\...\Run` value `Kiln` and the user
  `Path`, both from the setup window.
- Network: only the GitHub releases check of `tkhangg0910/kiln`.

## Licence

MIT, see [`LICENSE`](LICENSE): the original Clayo copyright, then the Rust port's and Kiln's.
Third-party crates and the embedded Chart.js / Mermaid files: [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
Citation metadata: [`CITATION.cff`](CITATION.cff). Background on the port: [`PORTING.md`](PORTING.md),
[`BENCH.md`](BENCH.md) (written for clayo-rust; names there are Clayo's).
