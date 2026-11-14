# Project structure

```
kilno/
├─ Cargo.toml            workspace (5 crates), release profile (fat LTO, panic=abort, strip)
├─ crates/
│  ├─ kilno-core/         logic, no UI
│  │  ├─ src/{agent,island,security,sessions,status,store,system}/   modules
│  │  ├─ src/*.rs        cli, crash, doc, git, images, launch, pages, pane_status, paths,
│  │  │                  screenshots, settings, settings_view, setup, sidebar, text, time
│  │  └─ tests/          integration tests, one file per area
│  ├─ kilno-pty/          ConPTY (lib.rs orchestration, sys.rs unsafe FFI, tests/conpty.rs)
│  ├─ kilno-term/         VT terminal: input, palette, tags, widget (+ examples/replay.rs)
│  ├─ kilno-island/       island window: anim, dib, glow, island, mascot, shots, text, view, sys/
│  │                     (+ examples island_demo, mascot_render)
│  └─ kilno-app/          kilno.exe
│     ├─ src/            app, pane, model, island_host, settings_page, setup_window, updater,
│     │                  doc_view, pages_open, status*, theme, fonts, image_chip, latency, win/
│     ├─ res/            icon, manifest, .rc (embedded by build.rs)
│     └─ tests/relay.rs
├─ assets/pages/         shared CSS/JS (Chart.js, Mermaid) and HTML templates for agent pages
├─ bench/                standalone benchmark tool (kilno-bench, own workspace and Cargo.lock)
├─ demo-data/            sample sessions and fixtures for demos
├─ scripts/
│  ├─ install.ps1        install/update into %LOCALAPPDATA%\Programs\Kilno
│  ├─ publish.ps1        builds publish\kilno-<version>-win-x64.zip and .sha256
│  └─ launchkit/         demo recording tooling
├─ .github/workflows/    ci.yml, release.yml
└─ docs/                 documentation (AGENTS.md; demo/ and assets/ hold media)
```

Root documents: `PORTING.md` and `BENCH.md` (historical, written for clayo-rust; names there are Clayo's), `LICENSE`, `THIRD_PARTY_NOTICES.md`, `CITATION.cff`.

## Boundaries

- `kilno-core`: must not depend on egui or Win32 windowing. Anything testable without a window belongs here. It must never write under `.claude` or `.codex`.
- `kilno-pty`: process and console plumbing only; no decoding of output.
- `kilno-term`: no process spawning; it is fed bytes and produces bytes to send.
- `kilno-island`: drawing and Win32 window plumbing; decisions (when to show) belong in `kilno-core::island::trigger`.
- `kilno-app`: wiring and UI; business rules should move to `kilno-core`.
- `bench/` is not in the workspace and never ships in `kilno.exe`.
