# Installation

## Prerequisites

- Windows 10 or 11, 64-bit.
- [Claude Code](https://docs.claude.com/en/docs/claude-code) and/or [Codex](https://github.com/openai/codex) (see [AGENTS.md](AGENTS.md); Kilno's setup window can install or locate them).
- `git` on PATH (used at run time for the branch/diff in the pane header).
- To build from source: Rust stable 1.85+ with the MSVC toolchain and the Visual Studio C++ build tools.

## Install a release

```powershell
irm https://raw.githubusercontent.com/tkhangg0910/kilno/main/scripts/install.ps1 | iex
```

This installs or updates into `%LOCALAPPDATA%\Programs\Kilno` and starts Kilno. If Kilno is running when you run it by hand, the script stops and asks you to quit first. Settings in `%LOCALAPPDATA%\Kilno` are kept.

Verify a downloaded zip (built by GitHub Actions with a build-provenance attestation):

```powershell
gh attestation verify kilno-<version>-win-x64.zip --repo tkhangg0910/kilno
```

## Build from source

```powershell
git clone https://github.com/tkhangg0910/kilno.git
cd kilno
cargo build --release     # target\release\kilno.exe
cargo test
cargo clippy --all-targets -- -D warnings
```

Run `target\release\kilno.exe`. No environment variables or database are needed.

## Build the release zip

```powershell
.\scripts\publish.ps1     # publish\kilno-<version>-win-x64.zip and .sha256
```

The version comes from `[workspace.package]` in `Cargo.toml`. Unzip into `%LOCALAPPDATA%\Programs` to get `%LOCALAPPDATA%\Programs\Kilno\kilno.exe`.

## Releasing

Push a `v*` tag on `main`. `.github/workflows/release.yml` checks that the tag is on main, runs `cargo test`, builds the zip, checks the zip name matches the tag, attests build provenance, and creates a draft release with the zip and its `.sha256`. Review the draft, write notes, publish.

## Verify it works

The first launch opens the setup window, which looks for Claude Code and Codex. After that the main window shows a sidebar of your sessions and the island appears at the top of the screen.
