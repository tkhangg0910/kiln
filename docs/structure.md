# Project structure

```
kiln/
├─ Cargo.toml            workspace (5 crates), release profile (fat LTO, panic=abort, strip)
├─ crates/
│  ├─ kiln-core/         logic, no UI
│  │  ├─ src/{agent,island,security,sessions,status,store,system}/   modules
│  │  ├─ src/*.rs        cli, crash, doc, git, images, launch, pages, pane_status, paths,
│  │  │                  screenshots, settings, settings_view, setup, sidebar, text, time
│  │  └─ tests/          integration tests, one file per area
│  ├─ kiln-pty/          ConPTY (lib.rs orchestration, sys.rs unsafe FFI, tests/conpty.rs)
│  ├─ kiln-term/         VT terminal: input, palette, tags, widget (+ examples/replay.rs)
│  ├─ kiln-island/       island window: anim, dib, glow, island, mascot, shots, text, view, sys/
│  │                     (+ examples island_demo, mascot_render)
│  └─ kiln-app/          kiln.exe
│     ├─ src/            app, pane, model, island_host, settings_page, setup_window, updater,
│     │                  doc_view, pages_open, status*, theme, fonts, image_chip, latency, win/
│     ├─ res/            icon, manifest, .rc (embedded by build.rs)
│     └─ tests/relay.rs
├─ assets/pages/         shared CSS/JS (Chart.js, Mermaid) and HTML templates for agent pages
├─ bench/                standalone benchmark tool (kiln-bench, own workspace and Cargo.lock)
├─ demo-data/            sample sessions and fixtures for demos
├─ scripts/
│  ├─ install.ps1        install/update into %LOCALAPPDATA%\Programs\Kiln
│  ├─ publish.ps1        builds publish\kiln-<version>-win-x64.zip and .sha256
│  └─ launchkit/         demo recording tooling
├─ .github/workflows/    ci.yml, release.yml
└─ docs/                 documentation (AGENTS.md; demo/ and assets/ hold media)
```

Root documents: `PORTING.md` and `BENCH.md` (historical, written for clayo-rust; names there are Clayo's), `LICENSE`, `THIRD_PARTY_NOTICES.md`, `CITATION.cff`.

## Boundaries

- `kiln-core`: must not depend on egui or Win32 windowing. Anything testable without a window belongs here. It must never write under `.claude` or `.codex`.
- `kiln-pty`: process and console plumbing only; no decoding of output.
- `kiln-term`: no process spawning; it is fed bytes and produces bytes to send.
- `kiln-island`: drawing and Win32 window plumbing; decisions (when to show) belong in `kiln-core::island::trigger`.
- `kiln-app`: wiring and UI; business rules should move to `kiln-core`.
- `bench/` is not in the workspace and never ships in `kiln.exe`.
