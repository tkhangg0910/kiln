# Testing

```powershell
cargo test                                   # all workspace crates
cargo clippy --all-targets -- -D warnings    # lints; also run in CI
```

CI (`.github/workflows/ci.yml`, windows-latest) runs `cargo build --release`, clippy, then `cargo test`. The release workflow runs `cargo test` before building the zip.

## What is covered

- `crates/kiln-core/tests/`: integration tests by area: `sessions`, `codex`, `status`, `strip`, `pane_status`, `island_trigger`, `settings`, `setup`, `agents`, `launch`, `pages`, `screenshots`, `sidebar`, `startup`, `update`, `userpath`, `permissions`, `prune`, `panefiles`, `env`.
- `invariants.rs` guards the rule that nothing writes under the Claude/Codex folders.
- `crates/kiln-pty/tests/conpty.rs`: real ConPTY spawn and I/O.
- `crates/kiln-app/tests/relay.rs`: the `--statusline` relay.
- Unit tests sit next to the code in `kiln-core/src`.

Tests use temp folders (`tempfile`), not your real data or agent folders. No coverage tool is configured. Performance measurements are done with `bench/` (`kiln-bench`); see the historical `BENCH.md`.
