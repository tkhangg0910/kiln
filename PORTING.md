# PORTING.md — Clayo C# → Rust native

> **Kiln note.** Written for clayo-rust (the Rust port of Clayo) before the fork; there `clayo-*`
> crates, `clayo.exe` and `%LOCALAPPDATA%\Clayo` are Kiln's `kiln-*`, `kiln.exe` and `%LOCALAPPDATA%\Kiln`.

> **Layout note (clayo-rust).** This document was written in the original repository, where the Rust
> workspace lived in `rust/` next to the C# app in `src/Clayo`. In clayo-rust the workspace is at the
> repository root (`rust/crates/...` is now `crates/...`), the page assets are in `assets/pages`, and
> the C# source and `checks/` are in the original project: https://github.com/YogiOnCode/Clayo

Bản đồ port Clayo 1.0.6 (C# / WPF / .NET 10 + WebView2, `src/Clayo`) sang Rust native cho Windows 10/11 x64,
không WebView. Nguồn yêu cầu: `RUST_PORT_PLAN.md`. Tài liệu này là hợp đồng của bản port: mỗi hành vi người
dùng thấy được có một mã (`MAIN-…`, `SES-…`, `STA-…`, `ISL-…`, `TRM-…`) và một ô `[ ]`. Một giai đoạn chỉ
xong khi các mã thuộc về nó được tick `[x]` (hoặc `[~]` = tạm hoãn đã duyệt, kèm lý do ở §6).

## Progress

611 mã: **599 `[x]`**, **12 `[~]`** (tạm hoãn đã duyệt: D1, D3, D4, D9, D10), **0 `[ ]` / `[c]`**. Theo vùng: MAIN 162/0, SES 116/0, STA 138/2, ISL 109/0, TRM 74/10 (`[x]`/`[~]`). Cập nhật ở giai đoạn 7c.

Đã đọc toàn bộ: `src/Clayo/**` (trừ thư viện JS đã minify), `checks/*.cs` (21 file), `docs/AGENTS.md`,
`README.md`, `scripts/*.ps1`. Phụ lục A–E (cuối file) là bản kiểm kê chi tiết theo từng vùng: bảng ánh xạ,
danh sách hành vi kèm `file:dòng`, định dạng dữ liệu, điểm khó, và danh sách assertion của từng check.

---

## 1. Stack đã chọn (và chỗ lệch so với đề xuất)

| Phần | Lựa chọn | Ghi chú / lý do |
|---|---|---|
| Workspace | `rust/` với `clayo-core`, `clayo-pty`, `clayo-term`, `clayo-island`, `clayo-app` | Như kế hoạch. `clayo-core` không phụ thuộc Windows (trừ module `system` có `#[cfg(windows)]`), test chạy mọi OS. |
| UI chính | `eframe`/`egui` | Như kế hoạch. |
| Backend render | **`glow` (OpenGL) thay vì `wgpu`**, có fallback `wgpu` (DX12 → WARP) | Lệch đề xuất. Lý do: ngân sách khởi động < 200 ms và RAM rảnh < 40 MB. Khởi tạo wgpu (DX12/Vulkan, shader compile, adapter enumerate) thường tốn 100–300 ms và 30–60 MB; glow/WGL khởi tạo ~30–60 ms, nhẹ hơn. Nếu tạo context OpenGL thất bại (VM, máy không driver chỉ có GL 1.1) thì thử lại bằng wgpu DX12/WARP; thất bại nữa thì MessageBox gốc + ghi crash.log (D10). Đo cả hai ở giai đoạn 2 và ghi vào BENCH.md; nếu glow không đạt độ trễ gõ phím < 5 ms thì chuyển mặc định sang wgpu. |
| PTY | ConPTY qua crate `windows` (tự bọc, ~300 dòng) | Không dùng `portable-pty`: cần đúng thứ tự teardown (TerminateProcess → ClosePseudoConsole ngoài UI thread, đọc tới EOF) và resize cùng-kích-thước để ép repaint; tự bọc giữ mọi `unsafe` trong `clayo-pty::sys` với comment `SAFETY`. |
| Phân tích VT | `alacritty_terminal` | Như kế hoạch. Có sẵn scrollback, selection (Simple/Block/Semantic/Lines), OSC 8, bracketed paste mode, DEC 2026, bảng độ rộng Unicode. |
| Widget terminal | Tự viết trong `clayo-term::widget`, vẽ bằng `egui::Painter` (rect nền theo run, glyph theo ô) | Tham khảo `egui_term`, không phụ thuộc nó (bám phiên bản egui cũ). Chỉ pane đang hiện được vẽ; pane ẩn vẫn đọc/parse. |
| Font | Nạp lúc chạy từ `C:\Windows\Fonts`, **memory-mapped** (mapping `unsafe` nằm trong lớp bọc Win32) để byte font không vào private working set. Chuỗi: Cascadia Mono/Code → Consolas (terminal); Segoe UI Variable/Segoe UI (UI); fallback theo glyph: Segoe UI Symbol → Segoe UI Emoji → Microsoft YaHei UI (CJK) → Yu Gothic UI → Malgun Gothic | Không nhúng font → exe nhỏ. Cascadia có đủ dấu tiếng Việt. Một atlas font egui dùng chung. Xem D7 về emoji màu. |
| Island | Cửa sổ Win32 riêng (`clayo-island`) bằng `windows`, `WS_EX_LAYERED|NOACTIVATE|TOOLWINDOW|TOPMOST`, `UpdateLayeredWindow` + `tiny-skia`; chữ bằng **`ab_glyph`** (đã có trong epaint) với chuỗi fallback theo glyph Segoe UI → Segoe UI Symbol/Emoji → font CJK | Như kế hoạch. tiny-skia không vẽ chữ nên dùng `ab_glyph` (không thêm crate, không parse trước mọi glyph). Island chạy trên luồng STA riêng có message loop (cần cho OLE drop). Glow là cửa sổ layered thứ hai luôn click-through, animation ≤ 30 fps, dừng sau 30 s. Menu chuột phải vẽ trong chính cửa sổ island (không `TrackPopupMenu`, vì nó buộc lấy focus). |
| Markdown | `pulldown-cmark` + `egui_commonmark` | Như kế hoạch. |
| HTML pages | Mở bản đã "seal" bằng trình duyệt mặc định | Như kế hoạch (xem D3). |
| JSON | `serde_json` (đọc khoan dung theo từng key bằng `Value`, ghi tay đúng thứ tự key) | Giữ tương thích `settings.json`, `names.json`, `parents.json`. |
| Theo dõi file | `notify` (ReadDirectoryChangesW) + debounce 600 ms tự viết | Như bản gốc. |
| Git | Gọi `git` CLI | Lệch đề xuất (`git2`). Lý do: giữ đúng ngữ nghĩa bản gốc (`rev-parse --abbrev-ref HEAD`, `diff --numstat --no-ext-diff --no-textconv`, `-c core.fsmonitor=false --no-optional-locks`, timeout 5 s), và `git2` kéo libgit2 (+~3 MB exe, build C). |
| Registry | `winreg` | Run key, `HKCU\Environment\Path` (giữ REG_SZ/REG_EXPAND_SZ, không expand). |
| Single instance | Named mutex `Local\CcxShell.SingleInstance` + pipe `\\.\pipe\CcxShell.Handoff.<SID>` (crate `windows`) | **Giữ nguyên tên** của bản C#, để bản cũ và bản Rust không chạy song song khi người dùng chuyển. |
| Clipboard / hotkey | `windows` (RegisterHotKey, AddClipboardFormatListener, CF_DIB/CF_HDROP/CF_UNICODETEXT) | PNG bằng crate `png`. |
| Ảnh (thumbnail chip) | `image` (chỉ bật png, jpeg, gif, webp, bmp) | |
| HTTP (updater) | **WinHTTP** qua crate `windows` (proxy hệ thống tự động, kho chứng chỉ Windows) | Kết nối mạng duy nhất (`api.github.com/.../releases/latest`). Không thêm dung lượng exe, giữ hành vi proxy/cert như .NET. Cài đặt vẫn do `install.ps1` (kiểm SHA-256) như bản gốc, nên không cần `self_update`. |
| Hộp thoại file | `rfd` (hộp thoại Win32 gốc) | |
| Đóng gói | Một `clayo.exe` (`windows_subsystem = "windows"`, `panic = "abort"`, `lto`, `opt-level = "s"` cho crate không nóng), icon + manifest (PerMonitorV2, longPathAware) qua `build.rs` | Asset pages (`clayo-page.css/js`, Chart.js, Mermaid) nhúng bằng `include_bytes!` và chỉ ghi ra đĩa khi mở một page. Zip release giữ bố cục `Clayo/clayo.exe`, tên `clayo-<ver>-win-x64.zip` + `.sha256` để `install.ps1` và nút Update của bản C# cài được bản Rust. |

## 2. Cấu trúc crate và ánh xạ tổng quát

```
rust/
  Cargo.toml                 workspace, profile release
  crates/
    clayo-core/              logic thuần, test được mọi OS
      agent/   detect, helper (lệnh install/sign-in, quote PowerShell), claude, codex
      launch                 LaunchPlan, ResolveShell, ScrubInheritedSession
      sessions/ claude, codex, tail (TranscriptTail), changed, watcher(debounce)
      store/   names, parents
      sidebar                SessionRow: tên, tuổi, bucket, sort, group, threading, filter, bind Codex pane
      pane_status            Scan/Answers/CountsAsWork/FromTimer/FromTurn
      status/  relay, relay_settings, model, parse, store, strip, reserve
      git                    numstat, is_repo, read, cache 3 s
      settings               ClayoSettings đọc khoan dung / ghi đúng thứ tự
      security/ ask_rules, own_rules
      island/  trigger (máy trạng thái), mention
      screenshots, images (PastedImages), pages (seal, hide_from_git)
      setup                  SetupScreen (văn bản + trạng thái)
      system/  login_startup, user_path, updater (#[cfg(windows)] cho phần registry)
    clayo-pty/               ConPTY: sys (unsafe), PtyProcess (spawn/read thread/write/resize/drop)
    clayo-term/              Term (alacritty) + pump + input keymap + selection + widget egui + image_tags
    clayo-island/            window, glow, mascot, anim, view, shots (hotkey/clipboard), drop (OLE), menu, sys
    clayo-app/               main (relay fast path), app, theme, main_window, sidebar, pane_header, chips,
                             viewer (doc/page), settings_ui, setup_window, single_instance, island_bridge
```

| Thư mục C# | Crate Rust | Phụ lục |
|---|---|---|
| `Program.cs`, `App.xaml(.cs)`, `MainWindow.xaml(.cs)`, `app.manifest` | `clayo-app` (+ logic sidebar thuần trong `clayo-core::sidebar`) | A |
| `Setup/*` | `clayo-core::setup` + `clayo-app::setup_window` | A |
| `System/*` | `clayo-core::system` + `clayo-app::single_instance` | A |
| `Agents/*`, `Sessions/*` | `clayo-core::{agent,launch,sessions,store,sidebar}` | B |
| `Status/*`, `Settings/*` | `clayo-core::{status,git,settings,security}` + `clayo-app::{status_strip,limits,settings_ui}` | C |
| `Island/*` | `clayo-core::{island,screenshots}` + `clayo-island` | D |
| `Terminal/*`, `Pages/*`, `Assets/terminal.html` | `clayo-pty`, `clayo-term`, `clayo-core::{pane_status,images,pages}`, `clayo-app::viewer` | E |
| `checks/*.cs` | `cargo test` trong crate tương ứng (bảng §5) | A–E mục CHECKS |

Bảng ánh xạ chi tiết từng file/lớp: mục **1. MAPPING** của mỗi phụ lục.

## 3. Bất biến và cách giữ

1. **Chỉ đọc `~/.claude` (hoặc `%CLAUDE_CONFIG_DIR%`) và `~/.codex` (hoặc `%CODEX_HOME%`).** Mọi đường dẫn tới hai
   thư mục này đi qua `clayo-core::paths::{claude_dir, codex_home}`, chỉ trả kiểu `ReadOnlyDir` không có API ghi.
   Test `invariants.rs` chạy toàn bộ luồng (scan, tail, relay settings, own rules) trên thư mục tạm, chụp
   (đường dẫn, kích thước, mtime) của cây trước/sau và assert không đổi. (Xem D1 về `clayo.rules`.)
2. **Dữ liệu riêng trong `%LOCALAPPDATA%\Clayo`**, cùng tên file và định dạng: `settings.json` (24 key camelCase,
   2 space, `.tmp` + rename), `names.json`, `parents.json`, `statusline.json`, `status\<guid>.json`, `shots\`,
   `no-login-start`, `crash.log`. Test round-trip đọc file do bản C# ghi (fixture) và ghi ra đúng byte.
3. **Mạng**: chỉ `GET https://api.github.com/repos/YogiOnCode/Clayo/releases/latest` (User-Agent `Clayo`,
   timeout 15 s). `install.ps1` (do PowerShell chạy) tải zip + checksum như bản gốc. Markdown không tải ảnh
   remote (D4). HTML page mở trong trình duyệt là việc của trình duyệt, CSP `connect-src 'none'` vẫn giữ.

## 4. Các điểm khó và cách làm

### 4.1 Status line relay (`--statusline` / `--pass`)
- `main()` kiểm `args[1] == "--statusline"` **trước mọi thứ** (không mutex, không log, không egui). `--pass` chỉ
  tính khi là `args[2]`. Đọc hết stdin (`read_to_end`), nếu `--pass` thì `write_all` y nguyên byte ra stdout
  (bỏ qua `BrokenPipe`), rồi lưu: parse JSON, `session_id` phải là GUID → ghi `status\<guid>.<pid>.tmp` rồi
  rename đè `<guid>.json`. Luôn exit 0. Exe là GUI subsystem nhưng handle stdin/stdout kế thừa từ pipe vẫn dùng
  được (`GetStdHandle`), sẽ có test tích hợp spawn `clayo.exe --statusline --pass` qua pipe.
- Tiêm vào Claude: mỗi lần mở pane Claude, ghi `%LOCALAPPDATA%\Clayo\statusline.json` (`statusline.dev.json`
  ở debug) chứa `statusLine` của người dùng (đọc từ `<claude_dir>\settings.json`, chỉ đọc) với `type:"command"`
  và `command: "'<exe với />' --statusline --pass | {\n<lệnh của họ>\n}"` (không có lệnh của họ thì chỉ
  `'<exe>' --statusline`), cộng `permissions.ask` khi có rule. Lệnh claude thêm `--settings '<path>'`.
  Không bao giờ ghi `~/.claude`.
- Phía app: `notify` theo dõi `status\*.json`, prune > 7 ngày, giữ bản tốt cuối cùng khi file dở dang; hạn
  mức tài khoản = báo cáo mới nhất có limit, trừ khi `%TEMP%\claude\statusline-usage-cache.json` mới hơn.
  Chi tiết: Phụ lục C §4.

### 4.2 Island không bao giờ lấy focus
- Tạo bằng `CreateWindowExW(WS_EX_LAYERED|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST, WS_POPUP)`;
  hiện bằng `SetWindowPos(SWP_SHOWWINDOW|SWP_NOACTIVATE|SWP_NOZORDER)` / `ShowWindow(SW_SHOWNOACTIVATE)`;
  trả `MA_NOACTIVATE` cho `WM_MOUSEACTIVATE` (dự phòng). Vẽ bằng `UpdateLayeredWindow` (alpha per-pixel, pixel
  alpha 0 không nhận click). `WS_EX_TRANSPARENT` bật cả cửa sổ ở Compact/Hidden và lúc trượt ra. Glow là cửa
  sổ thứ hai luôn `WS_EX_TRANSPARENT`, đặt ngay dưới island trong z-order.
- Không có cửa sổ "bắt mép": vị trí chuột lấy bằng `GetCursorPos` theo chu kỳ 50 ms (250 ms khi ẩn và chuột
  xa mép > 200 px) đúng như bản gốc; mỗi tick chỉ vài syscall, CPU ~0%. Máy trạng thái (`IslandTrigger`) là
  hàm thuần trong `clayo-core::island::trigger`, nhận `IslandInput` + đồng hồ, port nguyên ~110 assertion.
- Click vào pill gọi `Reveal()` của cửa sổ chính (`SetForegroundWindow` được phép vì click là input cuối của
  tiến trình). Esc: `GetAsyncKeyState` phát hiện cạnh, không nuốt phím.
- Animation chỉ chạy khi chuyển trạng thái; mascot/glow "thở" dừng sau 30 s (như bản gốc) và chỉ khi
  `SPI_GETCLIENTAREAANIMATION` bật.

### 4.3 Hotkey screenshot
- `RegisterHotKey(island_hwnd, 1, MOD_CONTROL|MOD_ALT|MOD_NOREPEAT, 'S')` khi setting bật; thất bại → Tip
  "Ctrl+Alt+S is taken by another app" và Settings báo phím bị chiếm. Khi Pick hiện: đăng ký thêm 1/2/3/N/Esc
  (id 10–14, không modifier) và bỏ đăng ký khi rời Pick (giữ đúng hành vi gốc). Tự đề nghị gửi:
  `AddClipboardFormatListener` + `GetClipboardSequenceNumber` để khử lặp.
- Gửi ảnh: chỉ lưu PNG lúc người dùng chọn (`%LOCALAPPDATA%\Clayo\shots\yyyy-MM-dd_HH-mm-ss[-n].png`, alpha bỏ),
  rồi ghi `ESC[200~<path>ESC[201~` vào PTY của pane (không Enter); pane mới thì làm prefill.

### 4.4 Branch phiên
- Claude: `& '<claude>' --resume '<parent>' --fork-session --session-id '<guid mới>' [--settings ...]`; ghi
  `parents.json[child] = parent` ngay lúc bấm (transcript fork không chứa liên kết).
- Codex: `& '<codex>' fork '<parent>'`; Codex tự chọn id; liên kết nằm trong `session_meta.payload.forked_from_id`.
  Pane Codex mới mang GUID giữ chỗ và được gắn (`FindStarted`) với transcript đầu tiên khớp: (fork) cùng
  parent hoặc (new) không parent + cùng thư mục, `Started` ∈ [since, until), chưa pane nào giữ, chọn cũ nhất;
  `until` = min(since của pane chưa gắn kế tiếp cùng loại, since + 10 phút). Bản Rust so sánh mọi thời điểm
  ở UTC (bản gốc dùng giờ địa phương — cùng kết quả, trừ lúc đổi giờ).
- Threading sidebar: depth tối đa 8 (chống vòng), ThreadOrder ghép `Stamp(when)+id+"/"` từ gốc.
- Mọi id được quote PowerShell (`'`, ‘ ’ ‚ ‛ nhân đôi) vì id đọc từ transcript.

### 4.5 Đọc transcript Claude và Codex
- Claude sidebar: `*.jsonl` đệ quy dưới `<claude_dir>\projects`, bỏ thư mục `subagents`; head 40 dòng (cwd,
  sessionId, prompt đầu), tail 96 KiB (`ai-title`, `last-prompt`, lọc chuỗi trước khi parse); cache theo
  (size, mtime); mở file với share read|write|delete (bản gốc có thể lỗi khi Claude đang ghi — sửa êm).
- Codex sidebar: `rollout-*.jsonl` dưới `<codex_home>\sessions`, dòng 0 phải là `session_meta`; tiêu đề từ
  `session_index.jsonl` (dòng cuối thắng); cache theo size; LastActivity = timestamp dòng cuối parse được;
  turn từ `task_started/task_complete/turn_aborted`; context theo công thức baseline 12 000; limit 5h/7d từ
  `token_count.rate_limits` (window 300/10080 phút).
- Trạng thái đèn: Claude lấy từ luồng output PTY (marker "API Error:", "Do you want to", Codex thêm
  "Would you like to"; `ESC[1C` = dấu cách; carry 16 byte; tick 400 ms, burst ≥ 3 chunk/900 ms = Working,
  im 1500 ms = Done; output trong 500 ms sau resize / 250 ms sau phím không tính). Codex: sau khi có transcript
  thì Working/Done lấy từ turn. Toàn bộ là hàm thuần trong `clayo-core::pane_status`, port `scan.cs`/`prompt.cs`.
- Tail pane đang mở (doc chip/page chip/image chip): đọc từ max(0, len−512 KiB), chỉ dòng trọn vẹn, tìm
  Write/Edit/MultiEdit (Claude) hoặc apply_patch/FileChange (Codex) tới `.md` trong thư mục / `.html` trong
  `.clayo\pages\`.

### 4.6 Terminal native
- PTY thread đọc 16 KiB → (1) `pane_status::scan` trên byte thô, (2) parse vào `Term` (alacritty) dưới
  `FairMutex`, (3) `ctx.request_repaint()` chỉ khi pane đang hiện. Repaint gom theo khung hình (tương đương
  batch 8 ms), tôn trọng DEC 2026 nên Claude vẽ lại toàn màn hình không nháy. `cat` 10 MB: parser thread riêng
  cho mỗi pane, UI chỉ khóa `Term` lúc vẽ → không treo.
- Phím: mã hóa kiểu xterm (alacritty keymap), Ctrl+C có selection = copy (không gửi ^C), Ctrl+V = paste text
  (bracketed nếu mode 2004, CRLF→CR) hoặc danh sách file (ảnh → bracketed path, file khác → quote), chuột/focus
  report theo mode. Selection: kéo, double (từ), triple (dòng), Shift+click, Alt+kéo (khối). Scrollback 5000.
- Pane ẩn không vẽ và không giữ tài nguyên GPU riêng (một atlas font egui dùng chung).

- **IME / composition** (Telex tiếng Việt của Windows, IME CJK): bật IME cho widget terminal (`ImeAllowed`), nhận
  `Ime::Preedit`/`Ime::Commit`, vẽ chuỗi preedit tại con trỏ, đặt cửa sổ candidate tại ô con trỏ
  (`ImeCursorArea`), gửi chuỗi commit dưới dạng UTF-8 vào PTY. xterm.js có sẵn hành vi này nên phải làm lại.
- **Dấu kết hợp**: egui không shaping. Ô có ký tự cơ sở + dấu kết hợp (NFD tiếng Việt như `e`+U+0302+U+0301,
  zero-width joiner) được hợp nhất (NFC) trước khi vẽ; nếu không có dạng NFC thì vẽ chồng dấu lên ô. Có test với
  "ế" dạng phân rã.
- **Đo độ trễ gõ phím**: từ lúc nhận sự kiện phím tới lúc present khung hình chứa ký tự echo. Repaint phản ứng
  theo sự kiện, vsync tắt hoặc present mode mailbox (sau vsync 60 Hz không thể đạt 5 ms).
- eframe không gọi `update()` khi cửa sổ ẩn: pipe server, luồng đọc PTY, tick trạng thái và island không được
  phụ thuộc vào `update()` (có test ở giai đoạn 3).

### 4.7 Chạy mà không có WebView
- Doc `.md`: egui_commonmark trong panel split (20–80 %, mặc định 50 %) hoặc full overlay, nút Split/Full/✕,
  theo dõi mtime và giữ vị trí cuộn khi cùng file.
- HTML page: D3.

## 5. Port các check sang `cargo test`

| Check C# | Test Rust | Ghi chú |
|---|---|---|
| agents.cs | `clayo-core/tests/agents.rs` | env + runner giả; phần runner thật (`cmd /c`) chỉ `#[cfg(windows)]` |
| codex.cs | `clayo-core/tests/codex.rs` | fixture trong thư mục tạm, `CODEX_HOME` giả |
| scan.cs, prompt.cs | `clayo-core/tests/pane_status.rs` | bộ mô phỏng 400 ms port nguyên |
| sessions.cs | `clayo-core/tests/sessions.rs` | watcher thật trên thư mục tạm |
| launch.cs, env.cs | `clayo-core/tests/launch.rs` | env.cs spawn `cmd` → `#[cfg(windows)]` |
| prune.cs, status.cs | `clayo-core/tests/status.rs` | `IsRepo(checkout)` dùng `git init` trong thư mục tạm |
| strip.cs | `clayo-core/tests/strip.rs` | |
| settings.cs, permissions.cs | `clayo-core/tests/{settings,permissions}.rs` | phần `SyncCodex` đổi theo D1 |
| panefiles.cs | `clayo-core/tests/panefiles.rs` | |
| pages.cs | `clayo-core/tests/pages.rs` | |
| island.cs, shots.cs | `clayo-core/tests/{island_trigger,screenshots}.rs` | |
| setup.cs, update.cs | `clayo-core/tests/{setup,update}.rs` | |
| startup.cs, userpath.cs | `clayo-core/tests/{startup,userpath}.rs` | `#[cfg(windows)]`, khóa test `HKCU\Software\ClayoTest*`, không chạm khóa thật |
| mascot.cs | `clayo-island/examples/mascot_render.rs` | không pass/fail; render PNG để so mắt |
| (mới) | `clayo-core/tests/invariants.rs` | không ghi vào `~/.claude`/`~/.codex` (CLAUDE_CONFIG_DIR/CODEX_HOME tạm) |
| (mới) | `clayo-app/tests/relay.rs` | spawn exe thật `--statusline --pass` qua pipe |

Mọi test dùng `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `LOCALAPPDATA`/thư mục dữ liệu được tiêm vào (không đọc biến
môi trường toàn cục trong test), nên không động vào cấu hình thật.

## 6. Quyết định và mục tạm hoãn (cần duyệt)

| # | Vấn đề | Đề xuất | Loại |
|---|---|---|---|
| D1 | Bản C# ghi `<CODEX_HOME>\rules\clayo.rules` khi bật "Also apply to Codex" (có hộp xác nhận). Trái bất biến "không ghi vào `~/.codex`". | Giữ bất biến: **không port phần ghi**. Công tắc "Also apply to Codex" vẫn hiện nhưng bị khóa, chú thích "Codex rules are read-only in this build"; key `alsoForCodex` vẫn đọc/ghi trong settings.json để không mất cấu hình. Không xóa file `clayo.rules` cũ (xóa cũng là ghi); "Your own rules" liệt kê nó như một file rules bình thường để người dùng thấy và tự xóa. Khi `alsoForCodex` = true và `clayo.rules` còn tồn tại, trang Security ghi "Still active in Codex; delete this file to turn it off" kèm link Open file. Không bao giờ xóa nó. Check `permissions.cs` phần `CodexRules` (sinh văn bản) vẫn port như hàm thuần; phần `SyncCodex` đổi thành test "không bao giờ ghi". | Bỏ (theo bất biến) |
| D2 | Sidebar bản gốc luôn đọc `%USERPROFILE%\.claude\projects`, bỏ qua `CLAUDE_CONFIG_DIR` (detector thì tôn trọng). | Tôn trọng `CLAUDE_CONFIG_DIR` (khác rỗng) ở mọi nơi, đúng như kế hoạch nêu; người không đặt biến này thấy y như cũ. | Sửa lỗi |
| D3 | HTML page cần Chart.js/Mermaid/clayo-page.js → cần engine trình duyệt. | **Duyệt.** Chip page, menu chuột phải liệt kê page, HideFromGit giữ nguyên. Bấm chip: ghi bản seal vào `%LOCALAPPDATA%\Clayo\pages\<hash>\` (kèm asset nhúng, link asset viết lại thành tương đối, CSP giữ `connect-src 'none'`, `form-action 'none'`), mở bằng trình duyệt mặc định. Phải kiểm CSP tải được asset `file://` trên Edge, Chrome, Firefox; nếu `'self'` không khớp thì inline asset vào file seal (`'unsafe-inline'` đã được phép). Thư mục `pages\` bị prune sau 7 ngày như `shots\`. **Không tự mở trình duyệt khi agent ghi page mới** (bản gốc tự mở trong pane); chip nháy thay vào đó. Hộp xác nhận "Open this link" trong app chỉ còn cho link trong doc và terminal. Mất: xem cạnh terminal, giữ vị trí cuộn/slide. | Tạm hoãn một phần |
| D4 | Doc markdown bản gốc tải ảnh `https:` từ xa (WebView tự tải). | Không tải (bất biến mạng); hiện alt text. Ảnh `data:` và ảnh file tương đối trong thư mục phiên được hiện. | Đổi nhỏ |
| D5 | Ctrl+V khi clipboard chỉ có bitmap (không phải file): bản gốc không làm gì (không gửi ^V). | Giữ đúng bản gốc (dùng phím dán ảnh riêng của agent, vd Alt+V). | Giữ |
| D6 | Pick đăng ký 1/2/3/N/Esc làm hotkey toàn cục tới 15 s. | Giữ đúng bản gốc. | Giữ |
| D7 | Emoji màu: egui không vẽ glyph COLR (Segoe UI Emoji). | **Duyệt tạm thời.** Emoji rộng đúng 2 ô, không ô vuông "tofu", đúng hình nhưng **đơn sắc**. Emoji màu được quyết lại ở giai đoạn 7 (không bỏ hẳn). **Giai đoạn 7 (2026-11-14): giữ đơn sắc** — egui/epaint không vẽ glyph COLR; đã kiểm trên màn hình: emoji rộng 2 ô, không tofu. Emoji màu là việc sau bản port (cần rasterizer COLR/DirectWrite). Chữ tiếng Việt, box drawing, CJK đầy đủ (font CJK ở §1). | Tạm hoãn |
| D8 | Kiểm tra bằng mắt `mascot.cs` so với `design/mascot/*.html` (không có trong repo). | Chỉ render PNG (example), không so tự động. | Tạm hoãn |
| D9 | `page-guide.md` + templates được ship nhưng không có code dùng; `clayo --page-check` không tồn tại. | Không port hành vi (không có hành vi). Vẫn nhúng asset cho D3. | Không áp dụng |
| D10 | WebView-only: F12 devtools (debug), tự tải lại khi renderer crash, DOM-fallback khi mất WebGL. | Không port các thứ chỉ có ở WebView. Thay cho DOM-fallback: nếu khởi tạo OpenGL thất bại thì thử wgpu DX12/WARP; thất bại nữa thì hiện MessageBox gốc và ghi crash.log. | Thay thế |
| D11 | Dòng thời gian Codex so ở giờ địa phương. | So ở UTC. | Sửa lỗi |
| D12 | Kết nối updater trỏ repo `YogiOnCode/Clayo` (bản C#). | Repo là hằng số build-time `CLAYO_UPDATE_REPO` (mặc định upstream `YogiOnCode/Clayo`); chủ dự án đặt nó sang repo của bản port trước bản phát hành Rust đầu tiên (nếu không, Update sẽ cài bản C# của upstream đè lên). Zip giữ bố cục để `install.ps1` hiện tại cài được. | Giữ + cấu hình |
| D13 | Ngân sách "CPU rảnh ~0%, không timer bận" và các poll của bản gốc (git 3 s, đồng hồ 30 s, ảnh 1 s, island 50/250 ms, tick trạng thái pane 400 ms, flush 8 ms). | Git/ảnh/đồng hồ chỉ chạy khi cửa sổ hiện (như gốc), dùng `request_repaint_after` thay vì repaint liên tục. Tick trạng thái 400 ms và flush 8 ms của mỗi pane thành **deadline do output kích hoạt** (pane im lặng thì không có timer). Con trỏ nhấp nháy chỉ repaint khi cửa sổ đang focus và hiện. Island giữ poll con trỏ (vài syscall/tick). Đo trong BENCH.md. | Giữ |
| D14 | Con trỏ thanh của terminal nhấp nháy mãi khi pane có focus (một repaint mỗi 600 ms, kể cả khi pane ngồi yên hàng giờ). | **Duyệt (2026-11-14, reviewer Fable 5.2).** Con trỏ vẫn nhấp nháy như bản C# (600 ms), nhưng sau 5 s không có input hay output trong pane thì đứng yên (hiện liền) và không xin repaint nữa; có hoạt động (phím, output, focus) là nhấp nháy lại. Pane rảnh = CPU 0 %. | Đổi nhỏ (đề xuất) |

**Duyệt (2026-11-13):** reviewer Fable 5.2, thay mặt chủ dự án theo yêu cầu tự động hóa hoàn toàn: *APPROVED WITH
CHANGES*. Đã áp dụng: fallback wgpu (D10), `ab_glyph` thay `fontdue`, WinHTTP thay `ureq`, font CJK + nạp font
memory-mapped, IME, dấu kết hợp, định nghĩa đo độ trễ, các hành vi còn thiếu (TRM-083/084, STA-137…140,
thư mục legacy CcxShell), và bổ sung D1, D3, D7, D12, D13 ở bảng trên. D1–D13 đều được duyệt (D10 theo phương án thay thế).

## 7. Thứ tự giai đoạn (theo kế hoạch)

1. `clayo-core` + port toàn bộ check thuần (Phụ lục B, C, D-trigger, E-pane_status/pages, A-setup/system).
2. `clayo-pty` + `clayo-term` + app tối thiểu một pane chạy `claude`.
3. Cửa sổ chính (sidebar, nhiều pane, resume, new, branch, rename, đóng = ẩn).
4. Status (relay trong cùng exe, header strip, footer limits 4 theme).
5. Island (`clayo-island`).
6. Hệ thống (setup, login, PATH, single instance, updater, crash log).
7. Doc chip, page chip, image chip, Settings đầy đủ; BENCH.md; README.

Mỗi giai đoạn: `cargo build --release`, `cargo clippy --all-targets -- -D warnings`, `cargo test` xanh,
tick các mã hành vi tương ứng, commit riêng trên nhánh `rust-port`.

---


# Phụ lục (kiểm kê chi tiết, tiếng Anh)

Trạng thái: `[ ]` chưa port, `[c]` logic đã port trong `clayo-core` (chờ nối UI), `[x]` đã port, `[~]` tạm hoãn đã duyệt (xem §6).
Một số hành vi xuất hiện ở hai phụ lục (vd sidebar ở A và B, status pane ở B và E) vì các vùng dùng chung
logic; khi port, các mã trùng được tick cùng lúc.


## Phụ lục A — App shell, main window, setup, system


Scope read fully: src/Clayo/Program.cs, App.xaml(+.cs), MainWindow.xaml(+.cs, 1775 lines), app.manifest, Setup/* (SetupWindow, SetupScreen, AgentRow), System/* (LoginStartup, SingleInstance, Updater, UserPath), scripts/install.ps1, scripts/publish.ps1, checks/setup.cs, startup.cs, update.cs, userpath.cs. Also peeked: Agents/AgentHelper.cs (used by setup + checks/setup.cs), ClayoSettings.NewSessionKind, StatusRelay consts, SessionNames/SessionParents file names. Paths below are relative to src/Clayo unless noted. Version in CcxShell.csproj: `<Version>1.0.6</Version>`, AssemblyName `clayo`, icon clayo.ico, StartupObject CcxShell.Program.

### 1. MAPPING

| C# file / class | Rust crate::module | Purpose |
|---|---|---|
| Program.cs / Program | clayo-app::main | Entry; `--statusline [--pass]` dispatches to relay before any UI loads; else run app |
| App.xaml.cs / App | clayo-app::app (startup) | Crash log, scrub inherited env, parse args, single instance, create main window, island, login-startup Keep, hard exit |
| App.xaml (resources) | clayo-app::theme | Color tokens, fonts, button/menu/scrollbar/expander styles -> egui Visuals + custom widgets |
| app.manifest | clayo-app build.rs (embed-resource / winres) | PerMonitorV2 DPI, longPathAware, Win10+ supportedOS |
| MainWindow.xaml(.cs) / MainWindow | clayo-app::main_window (+ ::sidebar, ::pane_header, ::folder_picker, ::images, ::chips) | Sidebar, pane host, header, footer, settings toggle, update flow, close-hides, quit |
| MainWindow.xaml.cs / SessionRow | clayo-core::sidebar::SessionRow (pure) + app view | Row label/subtitle/tooltip/bucket/branch depth/status text logic |
| MainWindow.xaml.cs / ApplyFilter, LinkThreads, Stamp, RecentFolderList, BindCodexPanes(order logic) | clayo-core::sidebar::{filter, layout, threads, recent_folders} | Pure sort/group/filter/thread ordering; testable |
| MainWindow.xaml.cs / BrushKeyConverter, BrushKeyFor | clayo-core::sidebar::status_key + clayo-app::theme | PaneStatus -> color key |
| MainWindow.xaml.cs / DarkCaption, PlaceOnPrimary | clayo-app::win32::{caption, placement} | DWM dark caption + acrylic + border color; center on primary work area |
| MainWindow.xaml.cs / FolderChoice, PickChoice records | clayo-core::sidebar / clayo-island::pick | Data records |
| Setup/SetupScreen.cs | clayo-core::setup::screen | Pure setup-window text/state machine (has checks) |
| Setup/SetupWindow.xaml(.cs) | clayo-app::setup_window | Setup egui viewport/window: detect, install, sign in, locate, options, Open Clayo |
| Setup/AgentRow.xaml(.cs) | clayo-app::setup_window::agent_row | One agent row widget (badge, pill, Use/Sign in/Install/Locate/Get Node) |
| Agents/AgentHelper.cs (dependency) | clayo-core::agents::helper | Install/sign-in command strings + powershell ProcessStartInfo |
| System/LoginStartup.cs | clayo-core::system::login_startup (registry via `winreg`, key path injectable) | HKCU Run value "Clayo" + opt-out marker |
| System/SingleInstance.cs | clayo-app::single_instance (or clayo-core::system) | Named mutex + per-user named pipe handoff |
| System/Updater.cs | clayo-core::system::updater (Newer/ScriptUrl/ReleaseUrl pure; NewerAsync + Start in app) | GitHub latest-release check, launch install.ps1 |
| System/UserPath.cs | clayo-core::system::user_path (key path injectable) | Add/remove exe folder to HKCU\Environment Path, broadcast WM_SETTINGCHANGE |
| scripts/install.ps1 | keep as-is (rust/scripts/install.ps1) | Download release zip, verify SHA-256, swap folder, start |
| scripts/publish.ps1 | rewrite as rust/scripts/publish.ps1 (cargo build --release) | Build zip `clayo-<ver>-win-x64.zip` with `Clayo/` root + `.sha256` |
| checks/{setup,startup,update,userpath}.cs | clayo-core tests (`#[cfg(windows)]` for registry ones) | See section 5 |

### 2. BEHAVIORS

#### Entry / command line
- [x] MAIN-001 `clayo --statusline` runs the status relay and exits, before any UI code loads (args[0] == "--statusline") — Program.cs:15
- [x] MAIN-002 `clayo --statusline --pass ...` relay with pass=true (args[1] == "--pass") — Program.cs:15; consts StatusRelay.cs:22-23
- [x] MAIN-003 Any other invocation runs the GUI app — Program.cs:16,21-26
- [x] MAIN-004 Unhandled UI-thread and AppDomain exceptions append to `%LOCALAPPDATA%\Clayo\crash.log` as `yyyy-MM-dd HH:mm:ss  <exception>\n\n`; log >1 MiB is moved to `crash.old.log` (overwrite) first; crash still proceeds (not swallowed) — App.xaml.cs:14-18,109-125
- [x] MAIN-005 On start, inherited Claude Code session env markers are removed from process env (SessionLauncher.ScrubInheritedSession) so every pane is top-level — App.xaml.cs:22
- [x] MAIN-006 `--background` arg (LoginStartup.BackgroundArg) = login start: no main window, island only, island Greet() — App.xaml.cs:25,62,67
- [x] MAIN-007 Folder = first arg that is an existing directory (any position); else, unless background, the current working directory — App.xaml.cs:30-31
- [x] MAIN-008 CWD is ignored (treated as "no folder") when it equals the exe's own folder (AppContext.BaseDirectory) or the System32 dir (Start menu/shortcut launches); compare is full-path, trailing separator trimmed, case-insensitive — App.xaml.cs:79-86
- [x] MAIN-009 `clayo` typed in Explorer's address bar works because Explorer runs the command with the current folder as CWD and the exe folder is on the user PATH (no App Paths registry key anywhere) — App.xaml.cs:27-31, UserPath.cs:7-10
- [x] MAIN-010 Main window opens on the folder if given (and auto-starts a new session in it), else on %USERPROFILE% with no session — App.xaml.cs:46-48, MainWindow.xaml.cs:381
- [x] MAIN-011 Second instance: hands folder (or "" when no folder; nothing when --background) to the running instance via pipe and exits — App.xaml.cs:33-40
- [x] MAIN-012 Running instance on receipt: "" -> Reveal(); path -> AdoptFolder(path) (on UI thread) — App.xaml.cs:51-55
- [x] MAIN-013 Windows sign-out (SessionEnding) performs a real Quit (panes closed) — App.xaml.cs:59
- [x] MAIN-014 Non-background start calls Reveal() (setup window first if setup needed) — App.xaml.cs:62
- [x] MAIN-015 Island window is created after main window; never keeps process alive (ShutdownMode=OnMainWindowClose) — App.xaml.cs:64-66, App.xaml:4
- [x] MAIN-016 Every start re-registers the Run key to the current exe unless opted out — App.xaml.cs:71, LoginStartup.cs:44-47
- [x] MAIN-017 On exit: dispose single instance, then hard `Environment.Exit(code)` so no stray foreground threads keep the mutex — App.xaml.cs:88-101

#### Single instance (System/SingleInstance.cs)
- [x] MAIN-018 Mutex name Release: `Local\CcxShell.SingleInstance`; Debug: `Local\CcxShell.SingleInstance.Dev`; created initiallyOwned; createdNew => we are primary — SingleInstance.cs:16,19,43-48
- [x] MAIN-019 Pipe name: `CcxShell.Handoff.<user SID>` (Debug `CcxShell.Handoff.Dev.<SID>`), i.e. `\\.\pipe\CcxShell.Handoff.S-1-5-...` — SingleInstance.cs:17,20,25-26
- [x] MAIN-020 Server: loop; inbound byte pipe, max 1 instance, async, CurrentUserOnly; read whole stream to EOF as UTF-8, Trim(), fire FolderReceived; IOException -> keep serving; UnauthorizedAccess -> stop; cancel on dispose — SingleInstance.cs:55-83
- [x] MAIN-021 Protocol: client connects (2000 ms timeout), writes folder path UTF-8 (no BOM effectively via StreamWriter default? .NET StreamWriter(Stream, UTF8) emits BOM — note: Encoding.UTF8 has BOM preamble; server StreamReader strips it), flushes, closes. No reply. Empty string = "just show window" — SingleInstance.cs:85-102
- [x] MAIN-022 Client calls AllowSetForegroundWindow(ASFW_ANY=-1) before sending so the primary can come to front — SingleInstance.cs:90,104-107
- [x] MAIN-023 Null folder (background launch) sends nothing — SingleInstance.cs:51
- [x] MAIN-024 Timeout/IO/Unauthorized errors on send are swallowed; second instance just exits — SingleInstance.cs:99-101

#### Window chrome / placement / theme
- [x] MAIN-025 Title "C L A Y O" (spaced; also Alt-Tab/taskbar label) — MainWindow.xaml:5
- [x] MAIN-026 Default 1280x820, min 760x420 — MainWindow.xaml:6-7
- [x] MAIN-027 Dark caption: DWMWA_USE_IMMERSIVE_DARK_MODE(20)=1, DWMWA_SYSTEMBACKDROP_TYPE(38)=3 (acrylic/transient), DWMWA_BORDER_COLOR(34)=0x00352C26 (#262C35); return values ignored; also applied to setup window — MainWindow.xaml.cs:387-428, SetupWindow.xaml.cs:71-75
- [x] MAIN-028 On first show the window is centered on the PRIMARY monitor work area (MonitorFromPoint(0,0, DEFAULTTOPRIMARY)), size clamped to work area, SWP_NOZORDER|SWP_NOACTIVATE (works around mixed-DPI CenterScreen bug) — MainWindow.xaml.cs:487-506
- [x] MAIN-029 Text rendering: TextFormattingMode=Display, UseLayoutRounding — MainWindow.xaml:11-12 (egui pixel-snaps text)
- [x] MAIN-030 Colors (App.xaml:12-29,73,291-303): TerminalBg #14171C, SurfaceBg #1B1F26, RaisedBg #222731, Hairline #262C35, TextPrimary #C8CDD6, TextMuted #7A828F, TextFaint #565E6B, MutedAA #8C94A1, Working #D4A24C, NeedsInput #5AA9E6, Done #6BBF8A, Failed #D2605F, Ended #5A6472, Dormant #39404B, Clay #D27556, BandGreen #6BBF8A, BandYellow #D9BE5C, BandOrange #E58C4E, BandRed #EC726C, DiffAdded #7CC596, DiffDeleted #E08480, StripRule #2E353F, LimitRule #262C35, MeterTrack #2A303A, ChipBg #1F242C, StepOn #A9B0BB, StepOff #39404B, MenuRim #323A46; footer band #171B21 — App.xaml, MainWindow.xaml:252
- [x] MAIN-031 Fonts: UI "Segoe UI Variable Text, Segoe UI"; mono "Cascadia Mono, Consolas"; icons "Segoe Fluent Icons, Segoe MDL2 Assets"; setup headline "Segoe UI Variable Display" — App.xaml:31-32, SetupWindow.xaml:21
- [x] MAIN-032 Single dark theme only for the chrome (Settings has a `Theme` passed to strip/limits views, owned elsewhere) — App.xaml, MainWindow.xaml.cs:1572-1573
- [x] MAIN-033 Button styles: QuietButton (transparent, border Hairline, radius 4, padding 10,5, hover RaisedBg+TextPrimary, disabled TextFaint/no border); GhostButton (h30, radius 6, border #2E353F, label opacity .75 -> 1 hover, wash of fg at .08 hover/.03 pressed, 160ms cubic ease, Segoe glyph in Tag before label); AccentButton (bg #2A303A, border #3A424E, fg #E6E9EE); ClayButton (bg Clay, fg #14171C, 14 semibold, h36, radius 8, hover #DC8466, pressed #B9654A, disabled #2A2F38/#6C7480) — App.xaml:34-69,92-170,309-350
- [x] MAIN-034 Switch (40x20 track, clay fill when on, knob slides 20px, grows 1.12 hover) and Tick (16px box radius 4, clay tick) checkbox styles; FocusRing is clay 1px radius 9, keyboard focus only — App.xaml:77-86,175-287
- [x] MAIN-035 Context menus: RaisedBg panel radius 8, rim #323A46, drop shadow, min width 176, items radius 5, hover #2D343F/#E6E9EE, gesture text #959CA8, check mark clay path, separator 1px MenuRim margin 10,4 — App.xaml:655-768
- [x] MAIN-036 Thin scrollbar 8px, thumb #333A45 (hover #4A5462), no arrows — App.xaml:411-444
- [x] MAIN-037 Splitter between sidebar and terminal: 7px grab band, 1px Hairline centered, hover/drag 2px Working color, cursor SizeWE — App.xaml:599-622, MainWindow.xaml:352-357

#### Layout sizes
- [x] MAIN-038 Columns: sidebar 310 (min 240, max 520) | 7 splitter | terminal * (min 360) — MainWindow.xaml:26-28
- [x] MAIN-039 Sidebar header: current folder leaf (14 semibold TextPrimary, ellipsis) over full path (mono 10.5 TextFaint), margin 14,14,14,8 — MainWindow.xaml:41-51, ShowFolder MainWindow.xaml.cs:591-595
- [x] MAIN-040 Search box margin 14,0,14,6, padding 8,6, caret Working, radius 4, hint "Search sessions" 12px faint — MainWindow.xaml:55-68, App.xaml:352-375
- [x] MAIN-041 Session item: 2px left selection rule (Working color when selected), content margin 34,7,10,7, hover #1F242C, selected #232935; row: 14px status column w/ 7px dot, name 12.5, subtitle 10.5 faint; branch indent Depth*15px — App.xaml:379-405, MainWindow.xaml:183-248, MainWindow.xaml.cs:132
- [x] MAIN-042 Section header (Open/Today/Earlier): chevron rotates 90 when expanded, label 11.5 semibold + count 11 faint, padding 10,7 — App.xaml:447-498, MainWindow.xaml:121-156
- [x] MAIN-043 Folder sub-header: margin 26,10,10,4, name 11 TextMuted + count — MainWindow.xaml:159-179
- [x] MAIN-044 Footer: margin 14,12,14,14; LimitsView, Update button, then New session | Open | (right) gear 30px wide glyph E713 size 15 — MainWindow.xaml:252-349
- [x] MAIN-045 Header bar: SurfaceBg, bottom hairline, margin 14,0; status dot 7px, title (margin 10,13, 12 TextMuted), status strip, chips, Attach, Branch, ✕ — MainWindow.xaml:366-488

#### Sidebar
- [x] MAIN-046 Rows are sessions from both agents' transcripts (only agents with Use on are scanned), plus panes started this window without a transcript yet — MainWindow.xaml.cs:617-656,1252-1262
- [x] MAIN-047 Rows persist across refresh (keyed by session id, case-insensitive) so status lights don't flicker; rows whose transcript disappeared are dropped unless open — MainWindow.xaml.cs:277,683-687
- [x] MAIN-048 Row name precedence: custom name > Claude ai-title > first prompt preview (unless "(no prompt yet)") > fallback title ("new session", or "new · <folder leaf>", or "branch of <first 8 of parent id>") — MainWindow.xaml.cs:62-66,1028,1120,1597
- [x] MAIN-049 Subtitle: open -> status text; else age — MainWindow.xaml.cs:68
- [x] MAIN-050 Age: "just now" (<1 min), "N min ago", "N h ago", "N d ago" (<7d), else "d MMM" — MainWindow.xaml.cs:81-92
- [x] MAIN-051 Status text: Working "running", NeedsInput "needs you", Done "done", Error "api error", Exited "shell closed", Starting "starting", else age — MainWindow.xaml.cs:176-185
- [x] MAIN-052 Status light color key: Working/Starting->Working amber, NeedsInput->blue, Done->green, Error->Failed red, Exited->Ended, none->Dormant (near invisible); tooltip = status text — MainWindow.xaml.cs:164-172, MainWindow.xaml:194-198
- [x] MAIN-053 Row tooltip: "Name\n<projectDir or folder>\n<sessionId>" + "\n\nBranched from: <parent name>" + "\n\nLast: <last prompt>" — MainWindow.xaml.cs:70-73
- [x] MAIN-054 Project = transcript project name, else leaf of folder — MainWindow.xaml.cs:75-77
- [x] MAIN-055 CC/CX tag at right of name (mono 9.5 semibold, opacity .85): "CX" #7DBBEB for Codex, "CC" #E39A7E for Claude; shown only when both agents are in use — MainWindow.xaml.cs:42-52,624,642, MainWindow.xaml:212-218
- [x] MAIN-056 Branch marker "↳" (11 faint) before name for any row with a parent; indent 15px per depth — MainWindow.xaml:219-224, MainWindow.xaml.cs:130-134
- [x] MAIN-057 Top-level sections: "Open" (has live pane), "Today" (last activity today), "Earlier"; sorted in that order; Open and Today expanded by default, Earlier collapsed; expand state remembered across rebuilds in a dict — MainWindow.xaml.cs:102-105,602-615
- [x] MAIN-058 Section header shows count of items — MainWindow.xaml:143-146
- [x] MAIN-059 Group combo: "Group: Folder" (default) / "Group: None"; with Folder, second-level grouping by Project — MainWindow.xaml:72-79, MainWindow.xaml.cs:760,767-769
- [x] MAIN-060 Sort combo: "Newest" (default) / "Oldest" / "Name" — MainWindow.xaml:80-89, MainWindow.xaml.cs:761
- [x] MAIN-061 Sort keys: BucketOrder asc; if grouped by folder: Name-sort -> Project asc; date sorts -> FolderRank (newest activity in that folder) desc for Newest / asc for Oldest, then Project asc tiebreak; finally Name asc (Name sort) or ThreadOrder asc (date sorts) — MainWindow.xaml.cs:773-795,690-694
- [x] MAIN-062 Branch threading: ParentId = transcript's parentId (Codex) else Clayo-recorded parents.json; depth = hops up through parents present in list, capped at 8 (cycle guard); ThreadOrder = concat over chain root..self of Stamp(when)+sessionId+"/"; Stamp = 19-digit zero-padded ticks, (MaxTicks - ticks) when newest-first — MainWindow.xaml.cs:838-879
- [x] MAIN-063 Search: trimmed text; matches Name contains / Project contains (case-insensitive) or SessionId starts-with — MainWindow.xaml.cs:749-758
- [x] MAIN-064 Search hint hides as soon as the box has focus or text; returns on blur if empty — MainWindow.xaml.cs:881-897
- [x] MAIN-065 List view only rebuilt when the layout signature (id|bucket|project-if-grouped|depth|parentId per row) changes; not rebuilt while any row is being renamed; selection restored without stealing focus; after rebuild scroll to top — MainWindow.xaml.cs:802-834
- [x] MAIN-066 Empty-state text when no transcript roots exist: "No transcripts found under ~\.claude or ~\.codex.\nStart a session and it will show up here." — MainWindow.xaml.cs:700-701
- [x] MAIN-067 Selecting a row that has a live pane switches to that pane; leaving a row cancels its rename — MainWindow.xaml.cs:909-918
- [x] MAIN-068 Double-click row or Enter (when not renaming) = Open: live pane -> activate; else resume (LaunchMode.Resume) in row folder (or current folder if gone) with row's agent — MainWindow.xaml.cs:923-950
- [x] MAIN-069 F2 on selected row starts inline rename — MainWindow.xaml.cs:932
- [x] MAIN-070 Context menu on list: "Open", "Rename" (gesture text F2), separator, "Reset name" — MainWindow.xaml:107-114
- [x] MAIN-071 Reset name clears custom name (names.json entry removed) — MainWindow.xaml.cs:1186-1192
- [x] MAIN-072 Rename box: in-row TextBox, pre-filled with custom name or current name, focused + select all; Enter commits, Esc cancels, losing focus commits; trimmed; empty or equal to ai-title => no override; updates header title if active — MainWindow.xaml.cs:189-214,1194-1239, App.xaml:626-647 (border Working color)
- [x] MAIN-073 Sidebar list not virtualized; horizontal scroll disabled — MainWindow.xaml:96-97

#### Footer actions
- [x] MAIN-074 "New session" (glyph E710, AccentButton) opens a folder popup above it: "Start a session in", up to 8 recent folders (current folder pinned first, then rows' folders by most recent activity, dedup case-insensitive, existing dirs only; each shows leaf + full path mono), separator, "Browse for a folder…" — MainWindow.xaml:264-324, MainWindow.xaml.cs:952-984
- [x] MAIN-075 Picking a recent folder or browsing (OpenFolderDialog title "Start a session in", initial = current folder) -> sets current folder + header and starts a new session there — MainWindow.xaml.cs:986-1017
- [x] MAIN-076 New session agent: settings NewSessionKind (Claude if Codex unused; Codex if Claude unused; else DefaultAgent Claude/Codex; Ask => null). Null -> context menu above New session button with "Claude Code" / "Codex"; dismissing starts nothing — MainWindow.xaml.cs:1024-1046, ClayoSettings.cs:56-64
- [x] MAIN-077 "Open" (glyph E8B7): OpenFileDialog "Open a file in a new session"; starts new session in the file's folder and, on that pane's first Done status, inserts the file path at the prompt (unsent) — MainWindow.xaml.cs:1054-1076
- [x] MAIN-078 Update button (glyph E896) hidden until a newer release is found (see Updater) — MainWindow.xaml:258-262
- [x] MAIN-079 Gear button (E713, tooltip "Settings"): left click opens menu above it: "Settings…" (gesture Ctrl+,), "Start at login" (checkable; state read fresh from registry on open; disabled if !CanWrite), separator, "Quit Clayo" — MainWindow.xaml:329-346, MainWindow.xaml.cs:1616-1637,1682-1684

#### Pane header / panes
- [x] MAIN-080 Header: status dot (same color mapping), pane title, status strip (HeadStrip), doc chip, page chip, images chip, "Attach files…", "Branch from here", "✕" — MainWindow.xaml:378-487
- [x] MAIN-081 Attach/Branch/✕ disabled when no pane; Branch also disabled until pane has a session id (Codex placeholder until bound) — MainWindow.xaml:468,476,485, MainWindow.xaml.cs:1319-1321,739,1169-1171
- [x] MAIN-082 "Attach files…": multi-select OpenFileDialog "Attach files" in active pane's cwd; inserts paths at prompt, refocuses terminal — MainWindow.xaml.cs:1086-1103
- [x] MAIN-083 "Branch from here": fork of active session (LaunchMode.Fork) in its cwd with its agent; if launcher allocated child id (Claude), record child->parent in parents.json immediately; new pane title "branch of <8 chars>" — MainWindow.xaml.cs:1105-1121
- [x] MAIN-084 "✕" closes active session: kill process, remove pane, notify island (notice cleared, working=false), may trigger pending update; row loses pane/status (falls to Today/Earlier); unbound Codex pane with no transcript is dropped; activates last remaining pane or shows empty state "Pick a session on the left, or start a new one." — MainWindow.xaml.cs:1123-1177, MainWindow.xaml:493-498
- [x] MAIN-085 Multiple panes alive simultaneously but only ONE visible at a time (others Collapsed, keep running) — MainWindow.xaml.cs:272-273,1312-1313
- [x] MAIN-086 Activating a pane: hides Settings if shown, MRU list update, title, enable buttons, hide empty state, paint dot, refresh git immediately, strip, images, chips, focus terminal — MainWindow.xaml.cs:1308-1331
- [x] MAIN-087 New pane gets a row immediately (status Starting) under Open before its transcript exists; Codex panes (no id) get a placeholder GUID and are re-keyed once their transcript appears (start-time + folder/parent match, within 10 min, not past the next same-kind pane's start) — MainWindow.xaml.cs:1244-1298,711-741
- [x] MAIN-088 Pane status change -> row status, header dot, island notices: NeedsInput->NeedsYou, Error->Error, Done only if previous was Working->Done, else null (clears); SessionWorking true on Working, false when leaving Working — MainWindow.xaml.cs:1270-1292
- [x] MAIN-089 Codex pane working/done driven from transcript turn info on each refresh — MainWindow.xaml.cs:629-632
- [x] MAIN-090 Doc chip (glyph F000): shown when active session wrote a .md; tooltip "<file>\n<path>\nClick to read it here, again to close it."; click toggles doc viewer; refocus terminal — MainWindow.xaml.cs:1471-1486
- [x] MAIN-091 Page chip (glyph E9D2): shown when session wrote pages in .clayo/pages; click toggles newest page; right-click menu lists the session's pages newest first (existing only), checked = open — MainWindow.xaml.cs:1489-1516
- [x] MAIN-092 New page written by session auto-opens (not for history on first read); open page/doc follows file; PageFolder.HideFromGit on write — MainWindow.xaml.cs:661-678
- [x] MAIN-093 Images chip (glyph EB9F + count): images pasted in active prompt since last sent prompt/pane open; Claude: files in paste cache dir for session; Codex: temp clipboard files matching pattern claimed per pane; only tags still in prompt text count; hover opens thumbnail popup (180px decode, 90 high, max 200 wide, "#n" label, wrap max 460); auto-hides 300ms after mouse leaves; new image flashes popup 1.5 s only if Clayo is active; click chip opens newest full-size overlay (#E614171C bg, margin 40, downscale only, ✕ / Esc / click closes, refocus terminal) — MainWindow.xaml.cs:1336-1468, MainWindow.xaml:418-461
- [x] MAIN-094 Status strip/footer limits: active pane's strip (Claude via StatusStore, Codex via transcript status) + git info; footer limits = active agent's account (Codex: newest across Codex transcripts; with no pane, Codex if Claude unused) — MainWindow.xaml.cs:1565-1586
- [x] MAIN-095 Reserve threshold crossing emits ReserveCrossed to island (warning only) — MainWindow.xaml.cs:1592-1595

#### Timers
- [x] MAIN-096 Git poll 3 s, only when window visible, for each pane cwd — MainWindow.xaml.cs:259,261,353-358
- [x] MAIN-097 Clock 30 s: refresh strip if visible; reserve check always — MainWindow.xaml.cs:263,360-365
- [x] MAIN-098 Image poll 1 s; image popup hide check 300 ms — MainWindow.xaml.cs:266-267,366-373
- [x] MAIN-099 Update check at load and every 12 h — MainWindow.xaml.cs:270,375-377
- [x] MAIN-100 On load: refresh sessions, start transcript watchers, status store, prune screenshots older than KeepFor — MainWindow.xaml.cs:345-350

#### Keyboard shortcuts
- [x] MAIN-101 Ctrl+, opens Settings (window-level preview, not when focus in terminal webview) — MainWindow.xaml.cs:1667-1673
- [x] MAIN-102 Esc closes Settings (unless Settings has a confirm to cancel first) — MainWindow.xaml.cs:1674-1678
- [x] MAIN-103 F2 rename, Enter open (sidebar) — MainWindow.xaml.cs:932-933
- [x] MAIN-104 Rename: Enter commit / Esc cancel — MainWindow.xaml.cs:1208-1217
- [x] MAIN-105 Image overlay: Esc closes — MainWindow.xaml.cs:1455-1458
- [x] MAIN-106 Setup: Enter = Open Clayo (IsDefault) — SetupWindow.xaml:125

#### Settings view
- [x] MAIN-107 Settings replaces the terminal side (over header + panes), panes hidden but running; popups closed; Refresh(settings, screenshotKeyTaken, updateTo) — MainWindow.xaml.cs:1645-1654, MainWindow.xaml:502
- [x] MAIN-108 Leaving Settings returns to active pane focus; picking/starting a session also leaves Settings — MainWindow.xaml.cs:1657-1662,1311
- [x] MAIN-109 On settings change: save settings.json, sync Codex ask rules, refresh strip, reserve check, notify island, re-show screenshot-key status; toggling UpdateCheck triggers a check (or clears offer) — MainWindow.xaml.cs:319-332
- [x] MAIN-110 Settings' "setup" request opens setup window; its Update request = Update button — MainWindow.xaml.cs:316-318

#### Close / quit / reveal
- [x] MAIN-111 Window X only hides (sessions keep running; island brings it back) — MainWindow.xaml.cs:1754-1761
- [x] MAIN-112 Quit (gear "Quit Clayo", island Quit, update, sign-out): stop watchers, dispose status, stop timers, close all panes, then app exits — MainWindow.xaml.cs:1602-1606,1762-1769
- [x] MAIN-113 No tray icon (the island is the background presence) — App.xaml.cs:24,64-67
- [x] MAIN-114 Reveal: if hidden and setup needed -> show setup window instead; else Show, restore if minimized (keeps prior maximized), Activate — MainWindow.xaml.cs:530-540
- [x] MAIN-115 NeedsSetup = !SetupDone || no in-use agent locatable (file check only, no run) — MainWindow.xaml.cs:546-555
- [x] MAIN-116 AdoptFolder(folder, prefill): if dir exists -> set current folder, Reveal; if main window is visible start a new session with optional prefill text typed (not sent) — MainWindow.xaml.cs:511-521
- [x] MAIN-117 Screenshot send (island): to chosen live non-exited pane -> reveal, switch, paste image path (Codex: track image); else new session in that pane's / last-used / current folder with pasted prefill — MainWindow.xaml.cs:1539-1554
- [x] MAIN-118 RecentSessions(max) for screenshot picker: MRU then others, distinct, live only; name/project/status ("new session" + cwd leaf if no row) — MainWindow.xaml.cs:1524-1532

#### Updater (System/Updater.cs + MainWindow)
- [x] MAIN-119 GET `https://api.github.com/repos/YogiOnCode/Clayo/releases/latest`, User-Agent "Clayo", 15 s timeout; reads `tag_name`; errors -> null; disabled in Debug — Updater.cs:24,35-49
- [x] MAIN-120 Newer: strip leading v/V, parse version, compare as (major, minor, max(build,0)) — Updater.cs:52-59
- [x] MAIN-121 If settings UpdateCheck off: no request, hide button, clear offer — MainWindow.xaml.cs:1698-1704
- [x] MAIN-122 Found: button text "Update to X.Y.Z", tooltip "Clayo X.Y.Z is out (you have A.B.C). Clayo closes, updates and opens again, once no session is working."; Settings shows it too — MainWindow.xaml.cs:1706-1712
- [x] MAIN-123 Click with panes open: confirm box title "Update Clayo", text "Clayo closes to update and opens again. Open sessions close and resume from the sidebar; any that are still working are waited for." OK/Cancel — MainWindow.xaml.cs:1729-1733
- [x] MAIN-124 Then waiting: button "Update waits for running sessions", tooltip "Clayo updates as soon as no session is working. Press to call it off."; clicking again cancels and re-checks — MainWindow.xaml.cs:1722-1738
- [x] MAIN-125 UpdateWhenIdle (called on click, every pane status change, pane close): when no pane is Working or NeedsInput -> Updater.Start(version) and Quit — MainWindow.xaml.cs:1742-1748
- [x] MAIN-126 Start: `powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "try { irm '<ScriptUrl>' | iex } catch { Write-Host $_ -ForegroundColor Red; pause }"` with env CLAYO_DIR=exe folder (AppContext.BaseDirectory, has trailing \), visible window — Updater.cs:62-73
- [x] MAIN-127 ScriptUrl = `https://raw.githubusercontent.com/YogiOnCode/Clayo/v{X.Y.Z}/scripts/install.ps1`; ReleaseUrl = `https://github.com/YogiOnCode/Clayo/releases/tag/v{X.Y.Z}` — Updater.cs:19-23

#### install.ps1 / publish.ps1
- [x] MAIN-128 Install dir: $env:CLAYO_DIR (trimmed \) or `%LOCALAPPDATA%\Programs\Clayo`; exe `clayo.exe` — install.ps1:9-11
- [x] MAIN-129 Run by hand while that Clayo runs: prints "Clayo is running. Quit it from the gear menu, then run this again." and stops — install.ps1:17-24
- [x] MAIN-130 Downloads latest release asset `*-win-x64.zip` and `<zip>.sha256`; first token of sha file must equal SHA-256 (case-insensitive -ne compare) else abort — install.ps1:27-41
- [x] MAIN-131 Staging `<parent>\Clayo-update-<guidN>`; expand; wait up to 10 s for running Clayo at that path, force kill, wait 10 s — install.ps1:13-15,42-52
- [x] MAIN-132 Swap: rename old dir to `<staging>-old`, move `<staging>\Clayo` -> dir, delete old; on failure restore old; always clean temp and start `clayo.exe` if present — install.ps1:57-71
- [x] MAIN-133 publish.ps1: reads <Version> from csproj; `dotnet publish -c Release -r win-x64 --self-contained -p:SatelliteResourceLanguages=en -o publish\Clayo`; asserts clayo.exe, Assets\terminal.html, Assets\xterm\xterm.js; zip `publish\clayo-<ver>-win-x64.zip` entries `Clayo/<rel path with />`; writes `<zip>.sha256` = "<lowerhex>  <zipname>" ascii no newline — publish.ps1:6-44

#### Login startup (System/LoginStartup.cs)
- [x] MAIN-134 Registry: HKCU `Software\Microsoft\Windows\CurrentVersion\Run`, value `Clayo` (REG_SZ) = `"<exe path>" --background` — LoginStartup.cs:13,15,17,64-69
- [x] MAIN-135 On by default: every start Keep() writes it unless opt-out marker `%LOCALAPPDATA%\Clayo\no-login-start` exists; only rewrites when value differs — LoginStartup.cs:44-47,68
- [x] MAIN-136 Set(on): delete marker + write; Set(off): create marker (empty) + delete value — LoginStartup.cs:49-62
- [x] MAIN-137 IsOn = value present — LoginStartup.cs:31-38
- [x] MAIN-138 Debug build never writes the real Run key (CanWrite false only for the real key) — LoginStartup.cs:24-29
- [x] MAIN-139 Exposed in gear menu, island menu, and setup window ("Start Clayo when I sign in", written immediately on click) — MainWindow.xaml.cs:1610-1627, SetupWindow.xaml.cs:173-176

#### PATH (System/UserPath.cs)
- [x] MAIN-140 HKCU\Environment value `Path`, read raw (DoNotExpandEnvironmentNames); kind kept REG_SZ if it was REG_SZ, else REG_EXPAND_SZ (also when missing) — UserPath.cs:51-57
- [x] MAIN-141 Entries split on ';', empty/whitespace dropped (so a trailing ';' disappears on write); add appends at end; remove deletes all matching; no-op if already in desired state — UserPath.cs:34-48,59-60
- [x] MAIN-142 Match: expand %VARS% in entry, trim spaces, trim quotes, trim trailing \ or /, case-insensitive — UserPath.cs:63-66
- [x] MAIN-143 After writing the real key: SendMessageTimeout(HWND_BROADCAST, WM_SETTINGCHANGE 0x1A, 0, "Environment", SMTO_ABORTIFHUNG, 2000) — UserPath.cs:47,69-78
- [x] MAIN-144 Folder = directory of the running exe; Debug never writes real key — UserPath.cs:19,25-30
- [x] MAIN-145 Written only on setup's "Open Clayo" (PathTick) — SetupWindow.xaml.cs:189-190

#### Setup window flow
- [x] MAIN-146 Window "Clayo setup", fixed content 760x524, padding 36,28,36,22, no resize, centered (on owner if main window visible), dark caption — SetupWindow.xaml:6-15,62, MainWindow.xaml.cs:571-576
- [x] MAIN-147 Only one setup window at a time (re-activates existing) — MainWindow.xaml.cs:565-569
- [x] MAIN-148 Layout: mascot 44x44 + headline (21 semibold #E6E9EE) + intro; agent card (Claude row, hairline, Codex row); options (picker, Start-at-login tick, PATH tick); footer: "Check again" | note | "Open Clayo" — SetupWindow.xaml:62-131
- [x] MAIN-149 Init: Use ticks from settings; picker selected from DefaultAgent (Codex/Ask/Claude); login tick = IsOn, disabled + " Only the installed Clayo can change this." if !CanWrite; PATH tick = (SetupDone or !CanWrite) ? currently on PATH : true; disabled + same suffix if !CanWrite — SetupWindow.xaml.cs:37-66
- [x] MAIN-150 On load: Detect() -> shows Checking view, async AgentDetector.DetectAsync(claudePath, codexPath); stale runs ignored (run counter) — SetupWindow.xaml.cs:68,77-86
- [x] MAIN-151 Mascot move: Peek while looking, Walk while installing/signing in, Jump when CanOpen, else Idle; only on change — SetupWindow.xaml.cs:96-109
- [x] MAIN-152 Picker visible only when both Ready; Options enabled only when CanOpen; Open enabled = CanOpen; Check again disabled while busy — SetupWindow.xaml.cs:115-118
- [x] MAIN-153 Toggling a Use tick recomputes view — SetupWindow.xaml.cs:39-40
- [x] MAIN-154 Install / Sign in: runs AgentHelper command in visible powershell window (`-NoProfile -ExecutionPolicy Bypass -Command "<cmd>; Read-Host 'Done. Press Enter to close'"`), shows Installing/SigningIn view, one at a time, waits for exit, then re-activates window and re-detects — SetupWindow.xaml.cs:127-147, AgentHelper.cs:26-42
- [x] MAIN-155 Locate…: OpenFileDialog "Where is <Name>?" filter "Programs (*.exe;*.cmd)"; runs `<file> --version`; failure -> note "That program didn't run. Pick <Name>'s claude.exe|codex.cmd."; success -> set ClaudePath/CodexPath (saved only on Open) and re-detect — SetupWindow.xaml.cs:153-171
- [x] MAIN-156 Open Clayo: DefaultAgent = picker choice if visible, else Claude if Claude Ready && Use else Codex; write PATH per tick; emit settings with SetupDone=true, UseClaude, UseCodex, DefaultAgent; close. MainWindow saves settings.json, syncs Codex rules, notifies island, refreshes Settings/sessions, Reveal() — SetupWindow.xaml.cs:178-200, MainWindow.xaml.cs:577-586
- [x] MAIN-157 Closing setup without Open changes nothing; next Reveal asks again — MainWindow.xaml.cs:559-562,587
- [x] MAIN-158 Agent row: badge "CC"/"CX" 36x36 radius 9 (lit colors only for Ready/SignIn/SigningIn: Claude bg #2A221F border #4A3329 fg #E39A7E; Codex bg #1C2530 border #2B3D52 fg #7DBBEB; unlit bg #1B1F26 border #2F3641 fg #8A929F); name 15 semibold; meta 12.5 MutedAA — AgentRow.xaml.cs:36-45, AgentRow.xaml:12-37
- [x] MAIN-159 Pill text/color: Looking… #D4A24C, Ready #6BBF8A, Not signed in #D4A24C, Installing… #D4A24C, Signing in… #D4A24C, Not installed #8A929F; pill bg = color at alpha 0x1F — AgentRow.xaml.cs:47-59
- [x] MAIN-160 Use tick visible only when Ready; Sign in button (amber filled) only when SignIn; fix line (install command box, Install, Locate…) only when Missing; Codex with no npm shows "Get Node.js first" link (https://nodejs.org) instead of Install — AgentRow.xaml.cs:61-68, AgentRow.xaml:41-58
- [x] MAIN-161 Install command displayed: Claude `irm https://claude.ai/install.ps1 | iex`; Codex `npm install -g @openai/codex` — AgentRow.xaml.cs:65
- [x] MAIN-162 Setup texts (SetupScreen.cs:30-112): see section 5 checks for exact strings

### 3. DATA

Registry (HKCU only):
- `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `Clayo` REG_SZ `"C:\...\clayo.exe" --background` (LoginStartup)
- `HKCU\Environment` value `Path` (REG_EXPAND_SZ, or REG_SZ if it already was) with exe folder appended (UserPath); followed by WM_SETTINGCHANGE "Environment" broadcast
- No App Paths, no file associations, no shell context-menu keys.

Files in `%LOCALAPPDATA%\Clayo\`:
- `settings.json` (ClayoSettings; must stay compatible) — saved by MainWindow on settings change / setup Done
- `names.json` (custom session names, id->name) — SessionNames
- `parents.json` (branch child->parent ids) — SessionParents
- `statusline.json` (status relay config) — StatusRelay.cs:42
- `shots\` (screenshots, pruned on load) — Screenshots.cs:16
- `no-login-start` (empty marker = user turned login start off)
- `crash.log`, `crash.old.log`
- Legacy `%LOCALAPPDATA%\CcxShell\WebView2` (WebView2 user data of the C# build, Terminal/TerminalPane.xaml.cs:189-192): the Rust build neither creates nor deletes it.
- (Rust, D3) `pages\<hash>\` sealed page copies for the browser, pruned after 7 days.

Install location: `%LOCALAPPDATA%\Programs\Clayo\clayo.exe` (or CLAYO_DIR); temp during update `%LOCALAPPDATA%\Programs\Clayo-update-<guid>{,.zip,.zip.sha256,-old}`.

Kernel objects: mutex `Local\CcxShell.SingleInstance` (Dev suffix `.Dev`), pipe `\\.\pipe\CcxShell.Handoff.<SID>` (Dev `CcxShell.Handoff.Dev.<SID>`). Keep names identical so old and new builds don't run side by side during migration (decide).

Network: only api.github.com releases/latest (app) + raw.githubusercontent.com + release asset downloads (install script, run in powershell).

### 4. HARD POINTS

- Relay fast path: `--statusline` must not initialize egui/wgpu/windows UI; check args first in main(), keep relay in a cold path (and ideally separate subsystem concern: exe is GUI subsystem `windows_subsystem="windows"` but relay must write stdout to Claude Code's pipe — works with inherited handles; verify stdout is a pipe, not console).
- Hide vs close: X must hide (egui: intercept `ViewportCommand::CancelClose` on close_requested, then `Visible(false)`); real quit only from Quit/update/sign-out (WM_QUERYENDSESSION/WM_ENDSESSION). eframe stops calling update() for hidden windows — pipe server, timers, pane readers, island must run off a non-UI thread or via request_repaint/winit proxy wakeups.
- Reveal from background: needs AllowSetForegroundWindow from the 2nd instance + SetForegroundWindow/restore-from-minimized-keeping-maximized (ShowWindow SW_RESTORE). Login start must create the app with the main viewport initially invisible.
- Hard exit at OnExit: process::exit after cleanup so ConPTY threads don't keep mutex alive.
- Pane hosting: WPF hosted one WebView2 (xterm.js) per pane, all alive, only active Visible; overlays (Settings) required hiding panes because HWND airspace; popups (images, menus) were separate windows. In egui all panes are just data + one renderer, so this simplifies; keep all PTYs alive and reading in background, render only the active one.
- Only one pane visible at a time (no split view). Settings replaces the terminal area.
- DPI: manifest PerMonitorV2; WPF CenterScreen misplaced on mixed-DPI, so the app manually centers on primary work area after creation (MonitorFromPoint(0,0)). winit has its own handling; still center on primary work area at first show, clamp size.
- DWM caption: dark mode + acrylic backdrop (attr 38 = 3) + border color; ignore failures on old builds. With egui/wgpu, opaque client area keeps acrylic only on the caption — fine with native decorations.
- Sidebar rebuild avoidance: WPF rebuilt list views; in egui immediate mode no flicker issue, but preserve: section expand state, selection by id, scroll-to-top only when layout signature changes, don't disturb inline rename.
- Codex pane binding: placeholder GUID rekeying (BindCodexPanes) with time window/ordering rules — port as pure logic in clayo-core with tests.
- Open-file flow waits for the pane's first Done before typing the path (status detection dependency).
- Update flow waits for all panes not Working/NeedsInput, re-evaluated on every status change; launching powershell with CLAYO_DIR then quitting; install.ps1 waits for process exit by exe path (`Get-Process clayo | Where Path -eq`) — Rust exe must keep name `clayo.exe` and zip layout `Clayo/clayo.exe` + `*-win-x64.zip` + `.sha256` so install.ps1 works unchanged (and old C# installs can update to Rust releases).
- Version comparison: release tags 3-part, C# assembly 4-part; Rust uses CARGO_PKG_VERSION (3-part) — keep Newer semantics.
- Debug vs Release guards: CanWrite false for real Run/Environment keys in Debug; mutex/pipe ".Dev" names; updater disabled in Debug. Map to `cfg!(debug_assertions)`.
- Pipe security: CurrentUserOnly + SID in name; Rust needs a security descriptor / `PIPE_REJECT_REMOTE_CLIENTS` and owner check (tokio named_pipe or windows crate). Message = raw UTF-8 until EOF (C# StreamWriter UTF8 writes a BOM; trim BOM on read for compatibility with an old client).
- Setup window is a separate top-level window (owner = main when visible) — egui multi-viewport (`show_viewport_deferred`) or a modal-in-window; it must be able to appear while the main window is hidden (first run / Reveal with NeedsSetup).
- Mascot animation in setup (Peek/Walk/Jump/Idle) shared with island renderer.
- Image popup only flashes when app is foreground (popups sit above other apps).
- Timers: C# polls git 3 s, clock 30 s, images 1 s, update 12 h; budget says ~0% idle CPU — use event-driven wakeups and skip work when hidden (git and strip already skip when hidden; reserve check runs hidden).
- ScrubInheritedSession must run before any PTY spawn (env vars removed process-wide).
- ShutdownMode OnMainWindowClose: island must never keep process alive.

### 5. CHECKS (port to cargo test)

#### checks/setup.cs (pure; clayo-core::setup)
Fixtures: claudeReady(Claude, exe C:\c.exe, ver "2.1.287 (Claude Code)", signedIn, sessions 42, projects 7, npm null); codexReady(Codex, C:\x.cmd, "codex-cli 0.160.0", signedIn, 9, 3, npm C:\npm.cmd); claudeMissing(no exe/ver, 0,0); codexMissing = codexReady w/o exe/ver/signedIn/sessions (npm kept); codexNoNode = codexMissing npm null; codexSignIn = codexReady signedIn=false; claudeSignIn = claudeReady signedIn=false.
- Checking(): title "Setting up Clayo"; both rows Looking; CanOpen false; OptionsEnabled false (also meta "Checking PATH and usual install folders", intro "Looking for Claude Code and Codex on this PC…", note "Takes about a second.")
- From(claudeReady,codexReady,T,T): title "Clayo is ready"; intro "Found both agents. Their sessions share one sidebar."; claude meta "v2.1.287 · 42 sessions in 7 projects"; codex meta "v0.160.0 · 9 sessions in 3 projects"; ShowPicker true; CanOpen true; note "Change any of this later in Settings."
- From(ready,ready,F,F).CanOpen == false
- From(ready,ready,F,T).CanOpen == true
- From(claudeReady,codexMissing,T,T): intro "Claude Code is all Clayo needs. Codex is optional."; codex meta "Optional. Needs Node.js."; ShowPicker false; note "Add Codex any time from Settings."
- From(claudeReady,codexNoNode,T,T).Codex.Meta == "Optional. Node.js not found."
- From(claudeReady,codexMissing,F,T).CanOpen == false
- From(claudeMissing,codexReady,T,T): intro "Codex is all Clayo needs. Claude Code is optional."; claude meta "Optional."; CanOpen true
- From(claudeMissing,codexSignIn,T,T): title "Almost there"; intro "Codex is installed but not signed in."; codex state SignIn; codex meta "v0.160.0 · Sign in once, then Clayo can use it"; CanOpen false; note "Sign in, then Check again."
- From(claudeSignIn,codexSignIn,T,T).Intro == "Both agents are installed but not signed in."
- From(claudeReady,codexSignIn,T,T): CanOpen true; intro "Codex is installed but not signed in. Clayo can start without it." (note "Sign in to Codex any time, then Check again.")
- From(claudeMissing,codexNoNode,T,T): title "Install one agent to start"; claude meta "Anthropic"; codex meta "OpenAI · Node.js not found"; CanOpen false; OptionsEnabled false (intro "Clayo runs your coding agent in its panes. Either one works.", note "Install installs it for you, in a window you can watch.")
- From(claudeMissing,codexMissing,T,T).Codex.Meta == "OpenAI · needs Node.js"
- Installing(From(claudeMissing,codexMissing,T,T), Claude): title "Installing Claude Code"; Claude row == (Claude, Installing, "Running the official installer…"); codex state Missing; CanOpen false; OptionsEnabled false (intro "Follow along in the PowerShell window. Clayo checks again when it closes.", note "Then sign in once, and Clayo is ready.")
- Installing(From(claudeReady,codexMissing,T,T), Codex): CanOpen false; codex meta "Running npm install -g @openai/codex…"
- SigningIn(From(claudeMissing,codexSignIn,T,T), Codex): title "Signing in to Codex"; codex state SigningIn (meta "Finish in the PowerShell window", note "Your browser may open to sign in.")
- AgentHelper.InstallCommand(claudeMissing) == "irm https://claude.ai/install.ps1 | iex"
- InstallCommand(codexMissing npm="C:\Program Files\nodejs\npm.cmd") == "& 'C:\Program Files\nodejs\npm.cmd' install -g @openai/codex"
- InstallCommand(codexNoNode) == None
- SignInCommand(claudeSignIn) == "& 'C:\c.exe' auth login"
- SignInCommand(codexSignIn) == "& 'C:\x.cmd' login"
- SignInCommand(claudeMissing) == None
- Window("x"): program "powershell.exe", UseShellExecute false, CreateNoWindow false (visible console)
- Window("x") last arg == "x; Read-Host 'Done. Press Enter to close'" (args: -NoProfile -ExecutionPolicy Bypass -Command <that>)
- claudeReady sessions=1 projects=1 meta == "v2.1.287 · 1 session in 1 project"
- claudeReady sessions=0 meta == "v2.1.287 · No sessions yet"
- ShortVersion("dev build") == "dev build" (regex `\d+(\.\d+)+` -> "v"+match, else input; null -> "")

#### checks/startup.cs (registry, #[cfg(windows)]; key `HKCU\Software\ClayoTest`, value "Clayo", marker `%TEMP%\clayo-check-no-login-start`; cleanup deletes key tree + marker)
- IsOn false before anything written
- Keep("C:\a\clayo.exe") -> IsOn true
- value == `"C:\a\clayo.exe" --background`
- Keep("C:\b\clayo.exe") -> value == `"C:\b\clayo.exe" --background` (follows moved exe)
- Set(false, b) -> value absent
- then Keep(b) -> IsOn still false (marker respected)
- Set(true, b) -> value == `"C:\b\clayo.exe" --background`
- then Keep("C:\c\clayo.exe") -> value == `"C:\c\clayo.exe" --background`
- ForThisUser().CanWrite == false in debug, true in release
- test-key instance CanWrite == true (even in debug)

#### checks/update.cs (pure)
- current = 1.0.0.0
- Newer("v1.0.1") == 1.0.1
- Newer("v1.2.0") == 1.2.0
- Newer("2.0.0") == 2.0.0 (no v)
- Newer("v1.0.0") == None (same release, 3 vs 4 parts)
- Newer("v0.9.9", 1.0.0.0) == None
- Newer("nightly") == None
- Newer(null) == None
- ScriptUrl(1.0.2.0) == "https://raw.githubusercontent.com/YogiOnCode/Clayo/v1.0.2/scripts/install.ps1"
- ReleaseUrl(1.0.5.0) == "https://github.com/YogiOnCode/Clayo/releases/tag/v1.0.5"

#### checks/userpath.cs (registry, #[cfg(windows)]; key `HKCU\Software\ClayoTestPath` value "Path"; cleanup deletes key tree; broadcast only for real key)
- no value: Contains("C:\clayo") false
- Set(true,"C:\clayo") on empty -> ("C:\clayo", REG_EXPAND_SZ)
- write "%USERPROFILE%\.local\bin;C:\tools;" (EXPAND); Set(true,"C:\clayo") -> "%USERPROFILE%\.local\bin;C:\tools;C:\clayo" EXPAND (rest as written, %VAR% unexpanded, trailing ; dropped)
- Set(true) again -> unchanged (no duplicate)
- Contains("C:\clayo") true
- Contains("C:\Clayo\") true (case + trailing slash)
- Set(false,"C:\clayo\") -> "%USERPROFILE%\.local\bin;C:\tools" EXPAND
- Set(false,"C:\clayo") again -> unchanged
- Contains(%USERPROFILE%\.local\bin expanded) true (entry with %VAR% matches)
- write "\"C:\Quoted Folder\";C:\x" as REG_SZ; Contains("C:\Quoted Folder") true
- Set(true,"C:\clayo") -> "\"C:\Quoted Folder\";C:\x;C:\clayo" stays REG_SZ
- ForThisUser().CanWrite false in debug / true in release
- test-key instance CanWrite true

## Phụ lục B — Agents and sessions


Scope read in full: src/Clayo/Agents/{AgentDetector,AgentHelper,ClaudeAgent,CodexAgent,IAgent}.cs, src/Clayo/Sessions/{ChangedFiles,CodexSessionStore,SessionLauncher,SessionNames,SessionParents,SessionStore,TranscriptTail}.cs, docs/AGENTS.md, checks/{agents,codex,scan,sessions,launch,prune,env,panefiles,permissions,prompt}.cs.
Also consulted (for consumers / status inference): MainWindow.xaml.cs (RefreshSessions, BindCodexPanes, ApplyFilter, LinkThreads, SessionRow, Fork_Click, StartPane), Terminal/TerminalPane.xaml.cs (Scan/FromTimer/FromTurn/Answers/CountsAsWork), Terminal/PastedImages.cs, Status/StatusStore.cs (Newest/Prune/KeepFor).
Paths below are relative to D:\Clayo\Clayo. Namespace in C# is `CcxShell.Core` throughout.

### 1. MAPPING

| C# file / type | Rust crate::module | Purpose |
|---|---|---|
| Agents/AgentDetector.cs `AgentKind` enum | clayo-core::agent | `Claude`, `Codex` |
| Agents/AgentDetector.cs `AgentInfo` record | clayo-core::agent::detect | Detection result: kind, exe?, version?, signed_in, data_folder, sessions, projects, npm? |
| Agents/AgentDetector.cs `AgentDetector` | clayo-core::agent::detect (pure, env+runner injected as closures/trait) | Locate exe (saved -> PATH -> known places), run `--version`/auth status, count sessions/projects |
| Agents/AgentDetector.cs `Run` | clayo-core::agent::proc (std::process, Windows CREATE_NO_WINDOW) | Run exe w/ 3 s timeout, kill tree, return stdout+stderr if exit 0 |
| Agents/AgentHelper.cs | clayo-core::agent::helper (+ spawn in clayo-app) | Install / sign-in PowerShell commands, PS single-quote quoting, visible PowerShell window |
| Agents/IAgent.cs `IAgent`, `ISessionSource` | clayo-core::agent (trait `Agent`), clayo-core::sessions (trait `SessionSource`) | Per-agent command lines; transcript source abstraction |
| Agents/ClaudeAgent.cs | clayo-core::agent::claude | Claude command lines (new/resume/fork) + `--settings` relay |
| Agents/CodexAgent.cs | clayo-core::agent::codex | Codex command lines (new/resume/fork) |
| Sessions/SessionLauncher.cs `LaunchMode`, `LaunchPlan`, `SessionLauncher` | clayo-core::launch | Build plan: shell cmdline, cwd, agent cmd, expected id; scrub env; PATH lookup |
| Sessions/SessionStore.cs `SessionInfo` | clayo-core::sessions::model | Row data for sidebar |
| Sessions/SessionStore.cs `SessionStore` | clayo-core::sessions::claude (parse) + watcher via `notify` crate | Read ~/.claude/projects/**/*.jsonl, incremental cache, debounce watcher |
| Sessions/CodexSessionStore.cs `TurnState`, `CodexSessionStore` | clayo-core::sessions::codex | Read CODEX_HOME/sessions rollout-*.jsonl, titles from session_index.jsonl, turn state, context/limits, FindStarted binding |
| Sessions/ChangedFiles.cs | clayo-core::sessions::changed | Set of changed paths from watcher + "all" flag |
| Sessions/SessionNames.cs | clayo-core::store::names | %LOCALAPPDATA%\Clayo\names.json (custom names) |
| Sessions/SessionParents.cs | clayo-core::store::parents | %LOCALAPPDATA%\Clayo\parents.json (Claude fork links) |
| Sessions/TranscriptTail.cs | clayo-core::sessions::tail | Incremental tail of open pane's transcript: doc chip, pages, last prompt time |
| (consumer) MainWindow SessionRow / LinkThreads / ApplyFilter / BindCodexPanes | clayo-core::sidebar (pure: naming, buckets, threading, sort keys, binding) + clayo-app UI | Sidebar model logic (port as pure functions so it is testable) |
| (consumer) TerminalPane.Scan/FromTimer/FromTurn/Answers/CountsAsWork | clayo-core::status::pane (pure) | Status light inference |
| (consumer) Terminal/PastedImages.cs | clayo-core::images | Find pasted images (checked by panefiles.cs) |
| (consumer) Status/StatusStore.Prune/Newest/KeepFor | clayo-core::status::store | Prune status files >7d; newest limits (checked by prune.cs, codex.cs) |

### 2. BEHAVIORS

#### Detection (setup window)
- [x] SES-001 Detect both agents concurrently off-thread (`DetectAsync`, Task.WhenAll) — Agents/AgentDetector.cs:34
- [x] SES-002 Locate order: saved path (if file exists) -> PATH -> known install places; first hit wins — Agents/AgentDetector.cs:97-103
- [x] SES-003 PATH search iterates names outer, PATH dirs inner: Claude tries `claude.exe` across the whole PATH, then `claude.cmd`; Codex tries `codex.cmd` then `codex.exe` — Agents/AgentDetector.cs:66,100-101
- [x] SES-004 PATH lookup: split on `;`, drop empty entries, strip `"` from each entry, Path.Combine(dir, exe), File.Exists; malformed entries skipped — Sessions/SessionLauncher.cs:92-108
- [x] SES-005 Claude known places in order: `%USERPROFILE%\.local\bin\claude.exe`, `%APPDATA%\npm\claude.cmd`, `%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe` — Agents/AgentDetector.cs:106
- [x] SES-006 Codex known places: `%APPDATA%\npm\codex.cmd`, `%LOCALAPPDATA%\Microsoft\WinGet\Links\codex.exe` — Agents/AgentDetector.cs:107
- [x] SES-007 Env var that is unset/empty makes its known place null (skipped); USERPROFILE missing -> "" root — Agents/AgentDetector.cs:109-112
- [x] SES-008 Found exe must run: `<exe> --version`; version = first non-empty trimmed line; if null (fail/non-zero/timeout) exe is reported as null — Agents/AgentDetector.cs:45-46,114-115
- [x] SES-009 Signed-in: Claude runs `<exe> auth status`, parses JSON from first `{` to end, `loggedIn == true`; any parse error -> false — Agents/AgentDetector.cs:49,118-128
- [x] SES-010 Signed-in: Codex runs `<exe> login status`; true iff any line (trimmed) starts with "Logged in" (ordinal); Codex prints on stderr — Agents/AgentDetector.cs:50,131-132
- [x] SES-011 Data folder: Claude `%CLAUDE_CONFIG_DIR%` else `%USERPROFILE%\.claude`; Codex `%CODEX_HOME%` else `%USERPROFILE%\.codex` (note: `env()` returning "" is NOT treated as unset here, unlike CodexSessionStore.ForThisUser) — Agents/AgentDetector.cs:52-54
- [x] SES-012 Claude counts: for each dir under `<data>\projects`, n = count of `*.jsonl` directly in it; sessions += n; projects++ if n>0 (subfolders like subagents NOT counted since non-recursive) — Agents/AgentDetector.cs:135-147
- [x] SES-013 Codex counts: every `rollout-*.jsonl` under `<data>\sessions` recursively is a session; projects = distinct `payload.cwd` of line 0 (TrimEndingDirectorySeparator, case-insensitive); unreadable/non-JSON lines still count as sessions — Agents/AgentDetector.cs:150-172
- [x] SES-014 IO/UnauthorizedAccess while counting -> (0,0), never fails setup — Agents/AgentDetector.cs:56-58
- [x] SES-015 npm (Codex only): PATH `npm.cmd`, then `%ProgramFiles%\nodejs\npm.cmd`; null for Claude — Agents/AgentDetector.cs:60
- [x] SES-016 Runner: UseShellExecute=false, CreateNoWindow, redirect stdout+stderr, default timeout 3000 ms, on timeout Kill(entireProcessTree) and return null; returns stdout+stderr concatenated only when ExitCode==0; Win32Exception (missing exe) -> null — Agents/AgentDetector.cs:72-94
- [x] SES-017 Locate (cheap, no run) used at every pane launch; fallback literal "claude"/"codex" when not found — Agents/ClaudeAgent.cs:16, Agents/CodexAgent.cs:11

#### Install / sign-in helper
- [x] SES-018 Claude install command: `irm https://claude.ai/install.ps1 | iex` — Agents/AgentHelper.cs:14
- [x] SES-019 Codex install command: `& '<npm.cmd>' install -g @openai/codex`; null (not installable) when npm not found — Agents/AgentHelper.cs:15
- [x] SES-020 Sign-in: Claude `& '<exe>' auth login`; Codex `& '<exe>' login`; null when exe not found — Agents/AgentHelper.cs:18-20
- [x] SES-021 Runs in visible `powershell.exe` with args `-NoProfile -ExecutionPolicy Bypass -Command "<cmd>; Read-Host 'Done. Press Enter to close'"` (ArgumentList quoting), never elevated/hidden; awaits exit — Agents/AgentHelper.cs:26-42
- [x] SES-022 PS quoting: wrap in `'...'`, double each of `'` U+2018 `‘` U+2019 `’` U+201A `‚` U+201B `‛` — Agents/AgentHelper.cs:48-57

#### Launch command lines (typed into pane's PowerShell)
- [x] SES-023 Shell: `"<full path of pwsh.exe>" -NoLogo` if pwsh.exe on PATH, else `powershell.exe -NoLogo` — Sessions/SessionLauncher.cs:30-35
- [x] SES-024 Pane spawns the shell, waits 400 ms, then types `AgentCommand + "\r"` (once) so exit lands on a prompt — Terminal/TerminalPane.xaml.cs:445-458 (consumer)
- [x] SES-025 Claude new: `& '<exe>' --session-id '<new guid>'[ --settings '<relay.json>']` — Agents/ClaudeAgent.cs:26,37-45
- [x] SES-026 Claude resume: `& '<exe>' --resume '<id>'[ --settings '<relay.json>']` — Agents/ClaudeAgent.cs:28
- [x] SES-027 Claude fork/branch: `& '<exe>' --resume '<parentId>' --fork-session --session-id '<new guid>'[ --settings '<relay.json>']` — Agents/ClaudeAgent.cs:30-31
- [x] SES-028 Claude `--settings` file path comes from StatusRelay.WriteSettings(selfExe, askRules) each launch; omitted if null or ProcessPath unknown; user's settings.json never written — Agents/ClaudeAgent.cs:18,43
- [x] SES-029 Ids are always PS-quoted (ids are read from transcripts: injection guard) — Agents/ClaudeAgent.cs:34, Agents/CodexAgent.cs:23
- [x] SES-030 Codex new: `& '<exe>'` (no id, no -C; shell already in folder) — Agents/CodexAgent.cs:20,27
- [x] SES-031 Codex resume: `& '<exe>' resume '<id>'` — Agents/CodexAgent.cs:23
- [x] SES-032 Codex fork: `& '<exe>' fork '<parentId>'` — Agents/CodexAgent.cs:25
- [x] SES-033 Exe path re-located on every launch (install/Locate during runtime honored) — Agents/ClaudeAgent.cs:4-5
- [x] SES-034 Plan: Resume/Fork without non-blank sessionId -> ArgumentException — Sessions/SessionLauncher.cs:67-68
- [x] SES-035 Plan: new guid (Guid.NewGuid, "D" format lowercase with dashes) only when agent.KnowsIdUpfront (Claude) and mode != Resume; Codex gets null — Sessions/SessionLauncher.cs:75
- [x] SES-036 Plan.ExpectedSessionId: Resume -> the given id; New/Fork -> the new guid (null for Codex, never the parent's) — Sessions/SessionLauncher.cs:87-88
- [x] SES-037 LaunchPlan fields: ShellCommandLine, WorkingDirectory, AgentCommand, ExpectedSessionId, Agent — Sessions/SessionLauncher.cs:17-22
- [x] SES-038 Resume cwd = row.Folder if it exists, else the app's current folder — MainWindow.xaml.cs:948
- [x] SES-039 Pane fallback titles: new `new · <folder leaf>`; fork `branch of <first 8 chars of parent id>`; resume = row name — MainWindow.xaml.cs:1028,1120,949

#### Environment
- [x] SES-040 At app start, remove from own process env: CLAUDECODE, CLAUDE_CODE_CHILD_SESSION, CLAUDE_CODE_ENTRYPOINT, CLAUDE_CODE_SESSION_ID, CLAUDE_CODE_MESSAGING_SOCKET, CLAUDE_CODE_MESSAGING_TOKEN, CLAUDE_PID (children inherit the scrubbed block); ANTHROPIC_*, CLAUDE_EFFORT left alone — Sessions/SessionLauncher.cs:48-63, App.xaml.cs:22

#### Claude session list
- [x] SES-041 Root: `%USERPROFILE%\.claude\projects` (UserProfile special folder) — does NOT honor CLAUDE_CONFIG_DIR (only detector does; docs/AGENTS.md:75 matches this) — Sessions/SessionStore.cs:66-71 (D2: the port honours CLAUDE_CONFIG_DIR)
- [x] SES-042 No root -> empty list — Sessions/SessionStore.cs:77
- [x] SES-043 Enumerate `*.jsonl` recursively; skip files whose parent dir name is `subagents` (case-insens.) — Sessions/SessionStore.cs:82,91-92
- [x] SES-044 Empty (0 byte) or missing file -> removed from cache/no row — Sessions/SessionStore.cs:98-102
- [x] SES-045 Cache invalidation: (size, LastWriteTimeUtc) equal -> reuse — Sessions/SessionStore.cs:105-106
- [x] SES-046 Parse failure (IO/permission, e.g. mid-write) keeps old row and re-queues file for next scan — Sessions/SessionStore.cs:109-114
- [x] SES-047 Result sorted by LastActivity descending — Sessions/SessionStore.cs:119
- [x] SES-048 Head: first 40 lines; skip empty/non-JSON; first `cwd`, first `sessionId`; preview = first `type=="user"` line with non-empty extracted text, shortened to 120; break when all 3 found — Sessions/SessionStore.cs:133-153
- [x] SES-049 User text extraction: skip if `isMeta:true`; `message.content` string -> Clean; array -> first block `type=="text"` with non-empty Clean(text) — Sessions/SessionStore.cs:259-284
- [x] SES-050 Clean: Trim; if starts with `<` and contains `>` -> ""; if starts with "Caveat:" -> "" — Sessions/SessionStore.cs:286-293
- [x] SES-051 Shorten: collapse all whitespace runs to single space; if > max chars, cut to max, TrimEnd, append `…` (U+2026) — Sessions/SessionStore.cs:295-299
- [x] SES-052 Tail: last 96 KiB; if not at 0, discard first partial line; only lines containing `"ai-title"` or `"last-prompt"` are parsed; last `type=="ai-title"` -> `aiTitle`, last `type=="last-prompt"` -> `lastPrompt` — Sessions/SessionStore.cs:188-239
- [x] SES-053 AiTitle = Shorten(Clean(title), 90) or null; LastPrompt = Shorten(Clean(prompt), 120) or null — Sessions/SessionStore.cs:234-238
- [x] SES-054 SessionId fallback = file name without extension — Sessions/SessionStore.cs:161
- [x] SES-055 ProjectDir fallback (no cwd) = mangled folder name (display only), else "(unknown)" — Sessions/SessionStore.cs:163-168
- [x] SES-056 Preview placeholder "(no prompt yet)" — Sessions/SessionStore.cs:175
- [x] SES-057 LastActivity (Claude) = file LastWriteTime (local) — Sessions/SessionStore.cs:178
- [x] SES-058 ProjectName = last path segment of ProjectDir (trim `\`/`/`), else ProjectDir — Sessions/SessionStore.cs:23-25

#### Codex session list
- [x] SES-059 Home: `%CODEX_HOME%` if non-empty else `%USERPROFILE%\.codex`; sessions dir = `<home>\sessions` — Sessions/CodexSessionStore.cs:26-31
- [x] SES-060 Enumerate `rollout-*.jsonl` recursively under sessions; any path not under `<sessions>\` or not named `rollout-*` ignored (watcher covers whole home) — Sessions/CodexSessionStore.cs:42,50-51
- [x] SES-061 Size 0/missing -> drop; cache key = size only (resume keeps old mtime) — Sessions/CodexSessionStore.cs:17-19,53-62
- [x] SES-062 Unparseable -> keep old row, re-queue — Sessions/CodexSessionStore.cs:64-69
- [x] SES-063 Titles re-read every scan from `<home>\session_index.jsonl`; last line per id wins; title = Shorten(Clean(thread_name), 90); ids case-insensitive — Sessions/CodexSessionStore.cs:76-102
- [x] SES-064 Line 0 must be `type=="session_meta"` with `payload`, and `payload.id` non-null, else not a session — Sessions/CodexSessionStore.cs:118-128
- [x] SES-065 ParentId = `payload.forked_from_id` — Sessions/CodexSessionStore.cs:125
- [x] SES-066 Started = `payload.timestamp` parsed (DateTimeOffset -> local) else file mtime — Sessions/CodexSessionStore.cs:126
- [x] SES-067 Preview: lines 1..60 until first UserText found; Codex's own `<environment_context>` etc. dropped by Clean — Sessions/CodexSessionStore.cs:133-138
- [x] SES-068 Model/effort from first `turn_context` in the head (only scanned while preview not yet found); overridden by tail's last turn_context — Sessions/CodexSessionStore.cs:136-144
- [x] SES-069 ProjectDir = payload.cwd else directory of file — Sessions/CodexSessionStore.cs:151
- [x] SES-070 LastActivity = last parseable line's timestamp in the 96 KiB tail, else Started (not file mtime) — Sessions/CodexSessionStore.cs:154,236
- [x] SES-071 LastPrompt = Shorten(last UserText in tail, 120) — Sessions/CodexSessionStore.cs:153,250
- [x] SES-072 Turn: last of event_msg payload.type task_started->Working, task_complete->Done, turn_aborted->Stopped — Sessions/CodexSessionStore.cs:243-245
- [x] SES-073 Status (strip) built only if context window >0 or model known: SessionStatus(id, model, cwd, used, size, percent, effort ?? "", fiveHour, sevenDay, updated = last ?? started) — Sessions/CodexSessionStore.cs:158-163
- [x] SES-074 Context: Baseline=12000; window<=12000 -> (used, window, window>0?100:0); else size=window-12000, mine=max(0,used-12000), left=round_away_from_zero(clamp((size-mine)*100/size,0,100)), percent=100-left — Sessions/CodexSessionStore.cs:169-183
- [x] SES-075 token_count: used = info.last_token_usage.total_tokens; size = info.model_context_window; rate_limits.primary/secondary with used_percent (double, floored to int), window_minutes (300 -> five-hour, 10080 -> seven-day, others ignored), resets_at (unix s, optional) — Sessions/CodexSessionStore.cs:279-299
- [x] SES-076 Sorted by LastActivity desc — Sessions/CodexSessionStore.cs:78

#### Watching
- [x] SES-077 FileSystemWatcher filter `*.jsonl`, recursive, NotifyFilter LastWrite|FileName|Size; Claude watches `projects` root, Codex watches whole `home` (for session_index renames) — Sessions/SessionStore.cs:305-310, Sessions/CodexSessionStore.cs:333-338
- [x] SES-078 Each Changed/Created/Deleted/Renamed event: add FullPath (and OldFullPath on rename) to ChangedFiles; (re)arm one-shot debounce of 600 ms; when it fires raise `Changed` (bg thread) -> UI RefreshSessions — Sessions/SessionStore.cs:313-321, Sessions/CodexSessionStore.cs:339-345, MainWindow.xaml.cs:344
- [x] SES-079 Watcher Error (buffer overflow): AddAll -> next scan enumerates everything; re-arm 600 ms — Sessions/SessionStore.cs:325-329
- [x] SES-080 ChangedFiles.Take(watching): returns (all = _all || !watching, paths) then clears; sets _all = !watching. Starts _all=true, so first scan after watcher starts is a full scan — Sessions/ChangedFiles.cs:11,22-31
- [x] SES-081 Full scan also drops cache entries no longer on disk; partial scan handles deleted files via fi.Exists==false — Sessions/SessionStore.cs:83-85,98
- [x] SES-082 StartWatching no-op if root missing or already watching; StopWatching disposes watcher+timer — Sessions/SessionStore.cs:303,332-338

#### Sidebar (consumer logic, MainWindow)
- [x] SES-083 Scan Claude (if UseClaude) + Codex (if UseCodex); tag rows `CC`/`CX` only when both enabled — MainWindow.xaml.cs:623-626, SessionRow.AgentTag
- [x] SES-084 Row name priority: custom name (names.json) > AiTitle > Preview (unless "(no prompt yet)") > FallbackTitle — MainWindow.xaml.cs SessionRow.Name
- [x] SES-085 Tooltip: `Name\n<ProjectDir or Folder>\n<SessionId>` + `\n\nBranched from: <parent name>` + `\n\nLast: <LastPrompt>` — SessionRow.Tip
- [x] SES-086 Subtitle: open pane -> status text, else age: <1 min "just now", <60 "N min ago", <24h "N h ago", <7d "N d ago", else `d MMM` — SessionRow.Age
- [x] SES-087 Buckets: Open (pane open, order 0), Today (When.Date==today, 1), Earlier (2) — SessionRow.Bucket
- [x] SES-088 When = Info.LastActivity, or now if no transcript yet — SessionRow.When
- [x] SES-089 Rows whose transcript vanished are removed unless open — MainWindow.xaml.cs:690-694
- [x] SES-090 Group by folder (GroupBy index 0) under bucket; folder groups ordered by FolderRank = newest When in that folder (desc for newest, asc for oldest; tie-break project name asc); sort=name -> project asc — MainWindow.xaml.cs:697-701,745-759
- [x] SES-091 Sort modes: 0 newest, 1 oldest, 2 name (alphabetical, branches not threaded) — MainWindow.xaml.cs:731,764-766
- [x] SES-092 Threading: ParentId = Info.ParentId (Codex) ?? parents.json; Depth = chain length-1 walking parents present in list, max 8 hops; ThreadOrder = concat over chain root..row of `Stamp(When)+SessionId+"/"`, Stamp = 19-digit ticks (MaxValue.Ticks - ticks for newest-first) — MainWindow.xaml.cs:826-865
- [x] SES-093 Filter text: name contains, project contains, or session id starts-with (case-insens.) — MainWindow.xaml.cs:719-726
- [x] SES-094 List view swapped only when layout string `id|bucket|project|depth|parent` per row changes, and not while a rename box is open — MainWindow.xaml.cs:774-780
- [x] SES-095 Empty state text "No transcripts found under ~\.claude or ~\.codex.\nStart a session and it will show up here." when neither root exists — MainWindow.xaml.cs:708-709
- [x] SES-096 Rename: F2; blank clears override; Enter opens; double-click opens; open row activates its pane — MainWindow.xaml.cs:925-950, SessionNames.cs:32-40
- [x] SES-097 Fork button: parent = active pane's SessionId (disabled until a Codex pane is bound); Claude child link written to parents.json immediately — MainWindow.xaml.cs:1104-1120
- [x] SES-098 New pane gets a row immediately (key = expected id, or a random placeholder guid for Codex), status Starting — MainWindow.xaml.cs:1244-1268
- [x] SES-099 Codex unbound panes rebound each refresh (see HARD POINTS) — MainWindow.xaml.cs:712-741
- [x] SES-100 Codex pane status from transcript each refresh: `pane.ShowTurn(info.Turn, info.LastActivity)` — MainWindow.xaml.cs:629-632

#### Open-pane transcript tail (doc / pages / image chips)
- [x] SES-101 Each refresh, each open pane with a transcript file: TranscriptTail(folder=pane cwd, since=pane OpenedAt, codex) created once and Read(file) — MainWindow.xaml.cs:661-686
- [x] SES-102 First read starts at max(0, len - 512 KiB); if mid-file the first (partial) line is skipped — Sessions/TranscriptTail.cs:18,59-64,77
- [x] SES-103 Only whole lines consumed (up to last `\n`); offset advances; partial last line re-read later — Sessions/TranscriptTail.cs:71-74
- [x] SES-104 File switch (different path, case-insens.) resets offset — Sessions/TranscriptTail.cs:59
- [x] SES-105 Lines considered only if containing `.md`/`.html` (case-insens.) or a prompt marker — Sessions/TranscriptTail.cs:87-97
- [x] SES-106 Claude prompt marker: contains `"type":"user"` and not `"tool_result"` and not `"isMeta":true`; Codex: contains `"role":"user"` and CodexSessionStore.UserText(line) non-null — Sessions/TranscriptTail.cs:92-96,109
- [x] SES-107 PromptAt = line `timestamp` (UTC) if >= since; earlier prompts ignored (image chip cleared only by new prompts) — Sessions/TranscriptTail.cs:105-109
- [x] SES-108 Docs from before pane opened still count (resumed session shows its last doc if file still exists) — Sessions/TranscriptTail.cs:103-104
- [x] SES-109 Claude paths: assistant `message.content[]` blocks `type=="tool_use"`, name Write/Edit/MultiEdit, `input.file_path` — Sessions/TranscriptTail.cs:131-142
- [x] SES-110 Codex paths: `payload.item.type=="FileChange"` -> each key of `payload.item.changes`; plus regex `\*\*\* (?:Add|Update) File: ((?:[^\\"]|\\.)+?)\\n` on raw line, captured group JSON-unescaped — Sessions/TranscriptTail.cs:147-167
- [x] SES-111 Path resolution: relative to pane folder; `.md` inside folder (prefix match on full path + `\`, case-insens.) and File.Exists -> Doc (latest wins); else `.html` inside `<folder>\.clayo\pages\` and exists -> page (move to end of list, PageWrites++) — Sessions/TranscriptTail.cs:113-127,170-179
- [x] SES-112 Read returns true iff a doc or page was seen in this read — Sessions/TranscriptTail.cs:52,81
- [x] SES-113 A newly written page auto-opens unless it was from the first (history) read; open doc viewer follows newest doc; `PageFolder.HideFromGit` called — MainWindow.xaml.cs:671-683

#### Pasted images (panefiles.cs)
- [x] SES-114 Claude root = `%CLAUDE_CODE_TMPDIR%` or %TEMP%, + `\claude`; session images dir = `<root>\<any project>\<sessionId>\images` (first match) — Terminal/PastedImages.cs:21-38
- [x] SES-115 Since(dir, sinceUtc, pattern="*"): image ext (.png .jpg .jpeg .gif .webp .bmp), mtime > since, ordered by numeric file stem (non-numeric last) then mtime; missing dir -> [] — Terminal/PastedImages.cs:41-53
- [x] SES-116 Codex pasted images: `%TEMP%\codex-clipboard-*` — Terminal/PastedImages.cs:17

### 3. DATA FORMATS

#### Read (never written)
- Claude transcripts: `%USERPROFILE%\.claude\projects\<mangled-cwd>\<uuid>.jsonl` (mangled e.g. `C--work-api`); `...\<uuid>\subagents\agent-*.jsonl` ignored (any dir named `subagents`).
  - Any line: `cwd` (string), `sessionId` (string), `type` (string), `isMeta` (bool), `timestamp` (ISO-8601 Z).
  - `type:"user"`: `message.content` = string | array of blocks `{type:"text",text}` / `{type:"tool_result",...}`.
  - `type:"assistant"`: `message.content[]` blocks `{type:"tool_use", name:"Write"|"Edit"|"MultiEdit", input:{file_path}}`.
  - `{type:"ai-title", aiTitle:"..."}` (rewritten as conversation grows; last wins, within final 96 KiB).
  - `{type:"last-prompt", lastPrompt:"..."}` (last wins).
  - Forked transcripts (`--fork-session`) contain copied parent lines with `sessionId` rewritten -> no parent reference.
- Claude detection/counting: `<CLAUDE_CONFIG_DIR or ~\.claude>\projects\*\*.jsonl` (non-recursive per project).
- `claude auth status` stdout: JSON, e.g. `{"loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty"}` possibly preceded by other text.
- `codex login status` (stderr): "Logged in using ChatGPT" / "Not logged in".
- Codex transcripts: `<CODEX_HOME or ~\.codex>\sessions\YYYY\MM\DD\rollout-<YYYY-MM-DDTHH-MM-SS>-<uuid>.jsonl`.
  - Line 0: `{"timestamp", "type":"session_meta", "payload":{"id","session_id","timestamp","cwd","forked_from_id"?, "forked_from_ordinal_exclusive"?, "originator","cli_version","base_instructions":{text ~19 KB}}}`.
  - `{"timestamp","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"..."}]}}` — user prompt (and Codex's own `<environment_context>...` context, dropped by Clean).
  - `{"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Add File: <path>\n..."}}`; code mode (0.162): `name:"exec"`, input JS with doubly-escaped patch.
  - `{"type":"event_msg","payload":{"type":"task_started"|"task_complete"|"turn_aborted"}}`.
  - `{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","changes":{"<abs path>":{"type":"add","content":...}}}}}`.
  - `{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens","output_tokens","total_tokens"},"model_context_window":N},"rate_limits":{"limit_id":"codex","primary":{"used_percent":12.5,"window_minutes":300,"resets_at":unix}|null,"secondary":{...}|null}}}`.
  - `{"type":"turn_context","payload":{"model":"...","collaboration_mode":{"mode":"default","settings":{"model","reasoning_effort":"high"|null}}}}`.
- `<CODEX_HOME>\session_index.jsonl`: `{"id":"<uuid>","thread_name":"...","updated_at":"..."}` one per naming; last per id wins. A fork's entry may carry the parent's title.
- Claude pasted images: `<CLAUDE_CODE_TMPDIR or %TEMP%>\claude\<project>\<session>\images\<n>.png`; Codex: `%TEMP%\codex-clipboard-*.png`.

#### Written by Clayo under %LOCALAPPDATA%\Clayo
- `names.json`: JSON object `{ "<sessionId>": "<custom name>", ... }`, WriteIndented, keys case-insensitive on load; blank name deletes key. Atomic: write `names.json.tmp`, then Move overwrite. Corrupt/unreadable -> start empty (and next Save overwrites). Dir created in ctor. — Sessions/SessionNames.cs
- `parents.json`: JSON object `{ "<childId>": "<parentId>" }`, same atomic write/indent/case rules; Set ignores blank ids and child==parent (case-insens.). Only written at fork time from Clayo (Claude only; Codex fork has no id upfront). — Sessions/SessionParents.cs
- (out of scope but referenced) `status\*.json` status files pruned when mtime older than 7 days (prune.cs); relay `--settings` JSON `{statusLine:{...,command:"'C:/.../clayo.exe' --statusline [--pass]"}, permissions:{ask:[...]}}` (permissions.cs); `settings.json` keys askOff[], askGroupsOn[], alsoForCodex, statusHeader, useCodex...
- Writes into CODEX_HOME: only `<CODEX_HOME>\rules\clayo.rules` (AskRules.SyncCodex, permissions area) — exception to "read-only ~/.codex" invariant; deleted when off; never creates a missing CODEX_HOME.

### 4. HARD POINTS

#### Session branching — Claude
- Clayo allocates the child id: `--resume <parent> --fork-session --session-id <newGuid>`. ExpectedSessionId = child guid, so the pane row exists and is addressable before any transcript.
- Claude's forked transcript rewrites `sessionId` on every copied line: no on-disk link to parent. Clayo writes `parents.json[child]=parent` right at Fork_Click (before pane starts). Forks made outside Clayo appear top-level.
- Sidebar parent = `Info.ParentId ?? parents.Get(id)`. Cycle guard: Set refuses self-parent; LinkThreads caps walk at 8 hops (MaxBranchDepth) so a cyclic parents.json can't spin. Parent not in list -> depth stops there (row depth 0 if direct parent missing).
- Fork requires active pane's SessionId (Codex panes have none until bound -> Fork button disabled until BindCodexPanes enables it).

#### Session branching — Codex
- `codex fork '<parent>'`; Codex picks child id; transcript `session_meta.payload.forked_from_id = parent` (so every Codex fork, even outside Clayo, threads). Fork does not copy history; it references the parent file (deleting parent breaks branches — docs/AGENTS.md:38-40).
- Binding a new/fork Codex pane to its transcript (FindStarted): candidates with `Started >= since` (pane start, local `DateTime.Now`) and `< until`, not in `taken` (SessionIds of all panes), and either (fork) `ParentId == parentId` (ordinal compare!) or (new) `ParentId == null` and same folder (TrimEndingDirectorySeparator, case-insens.); choose min Started (oldest first so 2 panes in one folder bind in order).
- `until` = min(Since of any later unbound pane with same ParentId and (fork or same folder), since + 10 min CodexStartLimit). Prevents older pane stealing newer pane's session; ignores sessions started from outside Clayo much later.
- Started = session_meta time = when codex started (before first message) — Codex writes nothing until first message, so row appears only after first prompt (docs/AGENTS.md:32-34). On bind: row re-keyed from placeholder guid to found id, pane.SessionId set.
- Timestamps compared in local time (Codex Started converted to LocalDateTime; since = DateTime.Now). Rust: use UTC consistently for both.

#### Transcript reading — Claude
- Sidebar scan: head 40 lines (cwd/sessionId/preview) + 96 KiB tail window (ai-title/last-prompt with substring prefilter). Mid-UTF8 seek: discard first line. Head opened with default StreamReader(path) (FileShare.Read) — may fail while Claude has file open for write -> treated as unreadable, row kept, retried next scan. In Rust open with share read|write|delete.
- Claude's working/needs-you/done is NOT from transcript: inferred from the PTY output stream (TerminalPane) + Claude status-line relay (StatusStore) for the strip:
  - Scan(chunk, carry, codex): prepend carry; replace every `ESC[1C` with a space (ConPTY encodes spaces as cursor-right); carry = last (maxMarkerLen-1)=16 bytes. If contains "API Error:" -> Error; else "Do you want to" -> NeedsInput; else (Codex pane only) "Would you like to" -> NeedsInput; else null. Byte-level ASCII search, no UTF-8 decode. Error beats prompt in same chunk. Only `ESC[1C` (not `ESC[5C`) maps to space.
  - Output time counts toward burst only if >500 ms after last resize and >250 ms after last key (CountsAsWork); recent-output queue capped at 12 timestamps.
  - Idle timer every 400 ms: burst = chunks in last 900 ms. FromTimer: if current is NeedsInput/Error and not answered -> no change; burst >= 3 -> Working; quiet > 1500 ms -> Done; else no change. On Done, type pending prefill.
  - Scan hit: set status, `_answered=false`, `_askedAt=DateTime.Now` (for NeedsInput).
  - Keys: Answers(keys) = !(len>2 && keys[0]==ESC && keys[1]=='[') — arrow keys/focus/mouse reports don't answer; lone ESC, Enter, digits do. Any key sets _keyAt.
  - Exited sticks (shell exit -> Exited, timer stops).
#### Transcript reading — Codex
- Sidebar scan per changed file (size changed): line 0 session_meta, head ≤60 lines for preview, 96 KiB tail parsed line by line (every line JSON-parsed; half-written last line fails parse and is skipped so prior line's timestamp stands).
- Status from transcript (exact): TurnState from last task_started/task_complete/turn_aborted in tail window. Each refresh: pane.ShowTurn(turn, LastActivity) sets `_turnKnown=true`; FromTurn(current, turn, at, askedAt, answered): Exited -> null; NeedsInput and (!answered or at <= askedAt) -> null (question stays: approval not in transcript, turn still writes lines); else Working if turn==Working else Done (Stopped -> Done).
- Once _turnKnown, the idle timer no longer changes status (only prefill after 1.5 s quiet). Before first transcript, Codex panes use the stream heuristics like Claude.
- Answer key on a _turnKnown pane in NeedsInput flips immediately to Working (Codex writes nothing while approved command runs).
- NeedsInput still detected from stream (Scan with codex=true: "Would you like to" / "Do you want to").
- Strip for Codex built from transcript (no status line relay): model/effort from last turn_context (effort "" if null), context per Codex formula, 5h/7d limits. Account limits = StatusStore.Newest: status with any 5h/7d limit and max Updated (whole record taken, so newest report wins; sessions without 5h/7d don't blank).
#### Other edge cases
- Claude cache key uses mtime+size; Codex size only (resumed Codex file keeps old mtime; LastActivity from last line timestamp).
- Codex watcher on whole home: session_index renames trigger refresh; titles recomputed every Scan regardless.
- Debounce 600 ms means many-line writes collapse to one refresh; Changed fires on background thread.
- TranscriptTail for Claude counts slash-command user lines (`<command-name>...`) as prompts (only tool_result/isMeta excluded) — keep parity.
- Codex patch regex parses JSON-escaped path so `C:\\notes` (escaped `\n` lookalike) not mistaken for newline; non-greedy until literal `\n` escape.
- Inside(): Path.GetFullPath(Path.Combine(folder, path)) — absolute paths pass through; `..` escapes rejected by prefix check; requires file to exist now (deleted docs ignored).
- AgentDetector `env("CLAUDE_CONFIG_DIR") ?? ...`: empty-string env var would be used as data folder (C# env returns null for unset only). Rust: treat empty as unset? Decide; parity says only null.
- SessionStore never honors CLAUDE_CONFIG_DIR (detector count vs sidebar may disagree). Flag as possible bug to preserve or fix deliberately.
- Guid format: lowercase "D" (8-4-4-4-12).
- Ids quoted in commands to prevent a crafted transcript id typing commands into PowerShell.

### 5. CHECKS

#### checks/agents.cs (AgentDetector, fake env + runner table keyed "exe|args", temp folder)
- nothing installed: no exe (Exe==null)
- nothing installed: not signed in
- nothing installed: no sessions (0)
- default data folder is ~\.claude (USERPROFILE\.claude)
- known place found without PATH (~\.local\bin\claude.exe)
- version is the first line ("2.1.288 (Claude Code)" from "…\n")
- claude signed in from JSON (multi-line loggedIn:true)
- PATH beats a known place; a .cmd counts (bin2\claude.cmd)
- claude signed out from JSON ({"loggedIn": false})
- saved path beats PATH
- a saved path that's gone falls back to PATH
- found but --version fails: not usable (Exe null)
- found but --version fails: no version
- codex in %APPDATA%\npm
- codex version "codex-cli 0.160.0"
- codex signed in ("Logged in using ChatGPT\r\n")
- CODEX_HOME is the data folder
- codex sessions: rollout files only = 4 (incl. non-JSON rollout-d; other.jsonl excluded)
- codex projects: same folder in any case or trailing slash counts once = 2 (C:\work\api, C:\Work\API\, C:\work\web)
- npm found in Program Files (pf\nodejs\npm.cmd)
- codex signed out: CodexSignedIn("Not logged in\n")==false
- CLAUDE_CONFIG_DIR is the data folder
- claude sessions = 3
- claude projects: empty folders don't count = 2
- no npm for claude
- claude status with a line before the JSON -> true
- claude status not JSON ("oops") -> false
- no output at all (null) -> false
- runner returns output: Run("cmd.exe","/c echo hi").Trim()=="hi"
- runner: non-zero exit is null ("/c exit 3")
- runner: too slow is null (ping 6s, timeout 500ms)
- runner: gives up on time (<3 s elapsed)
- runner: missing exe is null

#### checks/codex.cs (CodexSessionStore on temp CODEX_HOME)
- no sessions folder: (RootExists,Count)==(false,0)
- rollout files only: 4 rows (other.jsonl, rollout-broken {not json, rollout-empty excluded)
- newest first: Scan()[0] == resumed session b (last line 16:11:45 beats others)
- plain: Agent==Codex
- plain: folder from session_meta
- plain: title from session_index ("Confirm readiness")
- plain: preview skips Codex's own context ("hai are you ready")
- plain: last active is the last line (15:40:01.565Z local)
- plain: no parent
- resumed: last line beats the file time
- resumed: last prompt ("just testing resume, reply with ok")
- resumed: a later rename wins ("Check available options")
- fork: parent from session_meta (forked_from_id == a)
- fork: its own first prompt
- fork: the parent's title, as Codex gives it
- no index entry: no title (null)
- no index entry: preview = first prompt
- grown: new last prompt "one more" (size changed, mtime reset)
- grown: new last active 17:00:05
- half-written last line ignored (LastActivity unchanged)
- started: from session_meta
- new pane: the session started in its folder after it (folder with trailing `\`)
- new pane: not one started before it (since 15:39 > started 15:38:24)
- new pane: not one another pane has (taken={a}; folder lowercase)
- new pane: not a fork in the same folder (since 16:00 -> fork excluded, no others) -> null
- new pane: not another folder's
- fork pane: the session naming its parent (folder ignored for forks)
- new pane: not a session started after the next pane opened (until 15:38:00) -> null
- new pane: the newer pane gets it (since 15:38:00, until 15:48:00) -> a
- fork pane: nothing forked from another (parent b) -> null
- turn: complete is done
- turn: aborted is stopped
- no token_count, no model: Status null
- turn: started after the last complete is working
- strip: model from the latest turn_context ("gpt-6-luna")
- strip: effort from the latest turn_context ("high")
- strip: context as Codex shows it: (46800,258400) -> (34800, 246400, 14)
- strip: Context(49323,258400) == (37323, 246400, 15)
- strip: Context(11008,258400) == (0, 246400, 0)
- strip: a 5-hour window, floored: Limit(12, resets 1790000000) from 12.5
- strip: a 7-day window: Limit(40, 1790500000)
- strip: folder (Cwd)
- strip: a 30-day window (43200 min) fits neither: both null
- strip: no turn_context, no effort: Effort==""
- account: the newest report wins: Newest(...).FiveHour.Percent==12
- account: a session without 5h/7d windows does not blank them: SevenDay.Percent==40

#### checks/scan.cs (TerminalPane.Scan / FromTurn — pane status)
- plain output is nothing (ANSI colored text) -> null
- api error in one chunk -> Error
- permission prompt in one chunk -> NeedsInput
- api error split across two reads ("...API Er","ror: 529") -> Error
- prompt split across two reads -> NeedsInput
- prompt split byte by byte ("Do you want to") -> NeedsInput
- no false positive across a gap (20 X's between halves) -> null
- utf-8 box drawing is not a marker -> null
- error wins over prompt in one chunk -> Error
- prompt with ConPTY's cursor moves for spaces (`Do\e[1Cyou\e[1Cwant\e[1Cto\e[1Cproceed?`) -> NeedsInput
- a cursor move split across two reads (`Do\e[1Cyou\e[` + `1Cwant...`) -> NeedsInput
- other cursor moves are not spaces (`\e[5C`) -> null
- codex: run a command ("Would you like to run the following command?", codex) -> NeedsInput
- codex: make edits -> NeedsInput
- codex: network access ("Do you want to approve network access...") -> NeedsInput
- claude: 'would you like to' is just text (codex=false) -> null
- FromTurn(Done, Working, …, answered) -> Working ("turn started: working")
- FromTurn(Working, Done) -> Done
- FromTurn(Working, Stopped) -> Done ("turn aborted: done, nothing runs")
- a question stays for the tool call written before it: FromTurn(NeedsInput, Working, at=asked-1s, asked, answered=true) -> null
- typed, then a line after the question: FromTurn(NeedsInput, Working, asked+3s, asked, true) -> Working
- a closed shell stays closed: FromTurn(Exited, …) -> null

#### checks/sessions.cs (SessionStore + watcher, temp root)
- before watching: every file is read (Count==1; subagents excluded)
- subagent transcripts are not sessions (no "agent-1")
- first scan while watching: Count==1
- a new transcript appears (poll up to 5 s): Count==2
- a changed transcript is read again: appended last-prompt -> LastPrompt=="later"
- a deleted transcript goes: Count==1
- a new subagent transcript still is not a session (after 1 s): Count==1

#### checks/launch.cs (SessionLauncher.Plan + agents)
- new: picks an id (ExpectedSessionId is a GUID)
- new: passes it to claude: `& 'C:\Users\O''Neil\.local\bin\claude.exe' --session-id '<id>' --settings 'C:\s\relay.json'`
- new: in the folder (WorkingDirectory == C:\work)
- new: a fresh id each time
- resume: keyed on the existing id ("abc")
- resume: command `<exe> --resume 'abc'<relay>`
- fork: a new id for the child (GUID)
- fork: command `<exe> --resume 'abc' --fork-session --session-id '<id>'<relay>`
- no relay settings: no --settings: `& 'claude' --resume 'abc'`
- an id is quoted: id `x'; calc; '` -> `& 'claude' --resume 'x''; calc; '''`
- typographic quotes are doubled: Quote(`C:\Users\O’Brien‘‚‛`) == `'C:\Users\O’’Brien‘‘‚‚‛‛'`
- own ids, new: none expected (null)
- own ids, new: no id passed to NewCommand (null)
- own ids, fork: not the parent's id (null)
- own ids, resume: the existing id
- codex new: command `& 'C:\Users\O''Neil\AppData\Roaming\npm\codex.cmd'`
- codex new: no id yet
- codex new: Agent==Codex
- codex resume: `<cx> resume 'abc'`
- codex fork: `<cx> fork 'abc'`
- codex fork: no id yet
- claude: Agent==Claude
- fork without a parent is refused (ArgumentException)

#### checks/prune.cs (StatusStore.Prune, Status area)
- older than a week (KeepFor=7d, +1h): deleted
- from yesterday: kept
- not a status file (notes.txt, 400 days): left alone (only *.json)

#### checks/env.cs (SessionLauncher.ScrubInheritedSession)
- child inherits the marker before scrubbing (CLAUDE_CODE_CHILD_SESSION=1 visible to cmd child)
- child cannot see the marker after scrubbing
- configuration is left untouched (ANTHROPIC_MODEL=keep-me still visible)

#### checks/panefiles.cs (TranscriptTail, PastedImages)
Claude tail (folder=proj, since=opened):
- a doc that was deleted since: ignored (Read false)
- a doc from before the pane opened (resumed): seen (Doc=OLD.md)
- but a prompt from before then is not (PromptAt==MinValue)
- a doc written since: seen (true)
- as its full path (docs\PLAN.md)
- nothing new: nothing to say (false)
- a doc outside the folder: ignored
- a file that is not markdown: ignored (code.cs)
- a line still being written: waits (no trailing \n -> false)
- and is read once whole (Doc=HALF.md)
- no prompt yet
- a prompt you sent is its time, a tool result is not (PromptAt == sent)
- nor one of Claude's meta lines (isMeta:true ignored)
- no page yet (Page null)
- a page written in .clayo/pages: seen (pageA)
- and the doc stays the doc (HALF.md)
- an .html outside .clayo/pages: not a page (false)
- a page written again is the newest, listed once: Pages == "b.html,a.html"
- and each write is counted: PageWrites==3
Codex tail:
- codex: a patch adding a doc, by relative path (notes/README.md)
- codex: a path leaving the folder: ignored (../escape.md)
- codex: a patch by full Windows path (escaped backslashes)
- codex: a patch run from exec (code mode) -> via FileChange item (docs\scratch\codex-test.md)
- codex: a patch adding a page (.clayo/pages/a.html)
- codex: an .html outside .clayo/pages: not a page
- codex: its own context is not a prompt (<environment_context>)
- codex: a prompt you sent is its time
Pasted images:
- no paste yet: no folder (FindDir null)
- found under its project (root\C--proj\sess-1\images)
- only images since then, in tag order: "2.webp,10.png" (excludes old 1.png, notes.txt)
- a folder that is gone: none (0)
- codex: only its clipboard files since then, oldest first: "codex-clipboard-a.png,codex-clipboard-b.png"

#### checks/permissions.cs (AskRules / StatusRelay.SettingsJson / ClayoSettings / OwnRulesReader — Settings/Status area)
- every group off by default (AskGroupsOn=="")
- nothing asks by default (ClaudeAsk count 0)
- nothing unticked by default (AskOff=="")
- codex off by default (AlsoForCodex false)
- eight rules; ids are unique (8)
- every pattern, for Bash and PowerShell: count == sum(Claude patterns)*2
- push asks in Bash `Bash(git push*)`; in PowerShell `PowerShell(git push*)`; push with -C `Bash(git -C * push*)`; gh api writes `Bash(gh api *-X*)`
- switch off: no rules
- unticking tags takes its patterns away (none contain "git tag" or "* tag*"); and only its patterns (count-4); push still asks
- one group "publishing"; All == groups' rules flattened
- group off keeps its ticks (AskOff "tags"); group off with a rule unticked: no rules; back on, unticked rule stays off; unknown group on ("nope,publishing") ignored
- settings file: statusLine.command starts with `'C:/Clayo/clayo.exe' --statusline --pass`; keeps their padding=2; permissions.ask count == ask.Count; permissions has only `ask`; their statusLine object not mutated
- switch off: no permissions key; statusLine.command == `'C:/Clayo/clayo.exe' --statusline` (no user statusLine -> no --pass)
- old settings.json without new keys: groups off, all ticked, codex off
- saved settings load back equal; askOff is a JSON array (len 2); askGroupsOn is an array (len 1)
- odd askOff entries skipped: ["tags",3,"",null,"push","tags","a,b"] -> "push,tags"
- askOff not an array ignored -> ""
- unknown group in file ignored
- CodexRules: codex off -> null; first line starts "# Written by Clayo"; one `prefix_rule(` line per Codex prefix; contains `prefix_rule(pattern=["git", "push"], decision="prompt")` and `prefix_rule(pattern=["gh", "api"], decision="prompt")`; switch off -> null; tags unticked -> no `"tag"`; all unticked -> null
- SyncCodex: codex off -> nothing written; on -> rules\clayo.rules == CodexRules; tick rewrites; switch off deletes; codex off deletes; user's default.rules untouched; only 1 file left in rules; nonexistent CODEX_HOME not created
- OwnRulesReader.Claude: missing file -> Problem "No settings file yet."; decisions "allow allow ask deny"; rule text "Read(./.env)"; unrelated note ""; ticked allow note "Push code still asks first"; unticked -> "Push code runs without asking"; switch off -> same; File == path; broken JSON -> "Couldn't read this file."; non-array list skipped (0 rules)
- OwnRulesReader.Codex: no rules folder -> "No rules files yet."; Clayo's own file not listed (3 rules); decisions "prompt allow allow"; multi-line rule `["git", "push"]`; no decision = allow (`["npm", "test"]`); ticked allow note "Push code still asks first"; Codex off -> "Push code runs without asking"; File name "default.rules"

#### checks/prompt.cs (TerminalPane timer+scan simulation; 400 ms ticks, burst window 900 ms, queue 12)
- a dialog drawn in several reads stays needs you (burst then dialog, at 5000 ms) -> NeedsInput
- an api error drawn in several reads stays an error -> Error
- a plain turn still ends done -> Done
- an arrow key (`\e[B`) and its redraw keep it -> NeedsInput
- a focus report (`\e[I`) keeps it -> NeedsInput
- answered (`\r`): working while it runs (at 5600) -> Working
- answered: done when the turn ends (9000) -> Done
- answered no (`\e` alone) : done, though little is drawn after -> Done
- a second question after the answer needs you again -> NeedsInput
- typing a prompt (30 keys, 120 ms apart, echo +30 ms): not working while you type, so no Done after -> Done (stays Done)
- sending it: working once the reply streams -> Working
- codex: a line written while the approval is open keeps it: FromTurn(NeedsInput, Working, asked+3s, asked, answered=false) -> null
- codex: answered, the next line is working -> Working
- codex: answered, a finished turn is done -> Done

## Phụ lục C — Status and settings


Paths relative to `D:\Clayo\Clayo`. `src/` = `src/Clayo/`. Line numbers are the real file lines.
Files read in full: src/Status/{StatusRelay,StatusStore,StatusStrip,StatusStripView,GitInfo}.cs, src/Settings/{AskRules,OwnRules,ClayoSettings,SettingsPane.xaml,SettingsPane.xaml.cs}, checks/{status,strip,settings}.cs, plus checks/permissions.cs (tests AskRules/OwnRules/StatusRelay.SettingsJson), Program.cs, Agents/ClaudeAgent.cs, relevant MainWindow.xaml.cs wiring.

### 1. MAPPING

| C# file / type | Rust target | Purpose |
|---|---|---|
| Program.cs `Program.Main` (relay branch) | clayo-app `main.rs` (first thing, before any UI init) | `clayo --statusline [--pass]` dispatch, no UI load |
| Status/StatusRelay.cs `StatusRelay.Run/Save` | clayo-core `status::relay` (run(pass, stdin, stdout, dir)) | stdin -> (stdout passthrough) -> atomic save `%LOCALAPPDATA%\Clayo\status\<guid>.json` |
| StatusRelay `SettingsJson/WriteSettings/UserStatusLine` | clayo-core `status::relay_settings` | Build per-pane `--settings` JSON (statusLine wrapper + permissions.ask) |
| StatusRelay `ClaudeConfigDir`, `StatusDir` | clayo-core `paths` | `%CLAUDE_CONFIG_DIR%` or `~/.claude`; `%LOCALAPPDATA%\Clayo\status` |
| Status/StatusStore.cs `Limit`, `AccountLimits`, `SessionStatus` records | clayo-core `status::model` | Data types |
| StatusStore `Parse/ReadLimit/Effort/ParseUsageCache/Newest/Prune` | clayo-core `status::parse` (pure fns) | Parse statusline payload, usage cache, effort fallback, newest-limits |
| StatusStore class (watcher, dict, Limits) | clayo-core `status::store` (uses `notify`) | Watches status dir, keeps per-session latest, merges with usage cache |
| Status/GitInfo.cs `GitStatus`, `GitInfo` | clayo-core `git` (`ParseNumstat`, `is_repo`, `read`), cache in clayo-core `git::cache` | Branch + uncommitted +/- per folder, 3 s throttle, background reads |
| Status/StatusStrip.cs `Band`, `Meter`, `Strip`, `StatusStrip` | clayo-core `status::strip` | Pure formatting: bands, tokens, model name, reset countdowns, meters, header/footer composition |
| StatusStrip.cs `ReserveWatch` | clayo-core `status::reserve` | Once-per-window reserve-crossed detection |
| Status/StatusStripView.cs `StatusStripView` | clayo-app `ui::status_strip` (egui) | Draws header strip per theme |
| StatusStripView.cs `LimitsView` | clayo-app `ui::limits_footer` (egui) | Draws sidebar footer limits per theme |
| StatusStripView.cs `Draw` | clayo-app `ui::meter_draw` | Bar, ring, steps, chip, tint, percent, stale dimming |
| Settings/ClayoSettings.cs `ClayoSettings`, `StatusTheme`, `NewSessionAgent` | clayo-core `settings` (custom serde, lenient per-key load) | `%LOCALAPPDATA%\Clayo\settings.json` model, load/save |
| Settings/AskRules.cs `AskRule`, `AskGroup`, `AskRules` | clayo-core `security::ask_rules` | Static rule table; Claude permissions.ask list; Codex `clayo.rules` text + sync |
| Settings/OwnRules.cs `OwnRule`, `OwnRules`, `OwnRulesReader` | clayo-core `security::own_rules` (`regex`) | Read-only listing of user's Claude permissions + Codex prefix_rules, with notes |
| Settings/SettingsPane.xaml(.cs) | clayo-app `ui::settings` (egui) | Settings page: General / Status bar / Security / Agents (opens setup) / Island |
| Agents/ClaudeAgent.cs (`--settings` append) | clayo-core `agents::claude` | Appends `--settings '<path>'` to claude command |
| MainWindow.xaml.cs status wiring (ShowStatusBar, WarnReserve, timers) | clayo-app `main_window::status` | Glue: 3 s git poll, 30 s clock, redraw on Changed |

### 2. BEHAVIORS

#### Relay / entry
- [x] STA-001 `clayo.exe --statusline` as FIRST arg (exact, case-sensitive) runs the relay and returns its exit code without starting the app; `--pass` only honored as SECOND arg — src/Program.cs:15
- [x] STA-002 Relay reads ALL of stdin to EOF first (not streamed), then if `--pass` writes the identical bytes to stdout, then saves; always exit code 0 — src/Status/StatusRelay.cs:92-110
- [x] STA-003 Stdout write IOException (their command didn't read) is swallowed; save still happens — src/Status/StatusRelay.cs:105
- [x] STA-004 Without `--pass` the relay prints nothing — src/Status/StatusRelay.cs:17,98
- [x] STA-005 Save: parses JSON, takes `session_id`; non-JSON / missing / non-string / not-a-GUID id => nothing saved, silently — src/Status/StatusRelay.cs:119-123
- [x] STA-006 Save path `%LOCALAPPDATA%\Clayo\status\<guid>.json` (guid normalized to lowercase hyphenated "D" format); written to `<guid>.<pid>.tmp` then `File.Move(overwrite)`; on IO/Access error tmp is deleted — src/Status/StatusRelay.cs:125-136
- [x] STA-007 Status dir created on demand — src/Status/StatusRelay.cs:129
- [x] STA-008 Saved bytes are the raw stdin bytes (no re-serialization) — src/Status/StatusRelay.cs:130
- [x] STA-009 Per-pane settings file: `%LOCALAPPDATA%\Clayo\statusline.json` (release) / `statusline.dev.json` (DEBUG build) — src/Status/StatusRelay.cs:39-43
- [x] STA-010 Settings file regenerated on every Claude launch (new/resume/fork); only rewritten if content differs; returns null on IO/Access error (then no `--settings` arg) — src/Status/StatusRelay.cs:50-62
- [x] STA-011 Relay command = `'<exe path with \ -> / and ' -> '\''>' --statusline` — src/Status/StatusRelay.cs:78
- [x] STA-012 With user command: `<relay> --pass | {\n<their command>\n}`; without (missing, non-string or empty): just `<relay>` — src/Status/StatusRelay.cs:72,80
- [x] STA-013 Other statusLine keys of the user (padding etc.) copied verbatim; `type` forced to `"command"` — src/Status/StatusRelay.cs:71,79
- [x] STA-014 `permissions.ask` added only when ask list non-empty; otherwise no `permissions` key — src/Status/StatusRelay.cs:82-84
- [x] STA-015 User's `<ClaudeConfigDir>\settings.json` `statusLine` only read; parse/IO failure or non-object => treated as no user statusLine — src/Status/StatusRelay.cs:139-147
- [x] STA-016 ClaudeConfigDir = `%CLAUDE_CONFIG_DIR%` if non-empty else `%USERPROFILE%\.claude` — src/Status/StatusRelay.cs:30-33
- [x] STA-017 Claude command gets ` --settings '<path>'` (PowerShell single-quote; `'` and smart quotes ‘ ’ ‚ ‛ doubled) — src/Agents/ClaudeAgent.cs:43, src/Agents/AgentHelper.cs:48-56
- [x] STA-018 WriteSettings uses `Environment.ProcessPath` (own exe) and current AskRules.ClaudeAsk(settings) — src/Agents/ClaudeAgent.cs:18

#### Status store
- [x] STA-019 Payload accepted only if whole JSON object with string `session_id` that parses as GUID; else ignored — src/Status/StatusStore.cs:174-181
- [x] STA-020 Context size = `context_window.context_window_size` if number > 0 else 200000 — src/Status/StatusStore.cs:184
- [x] STA-021 Context used = input_tokens + cache_creation_input_tokens + cache_read_input_tokens from `context_window.current_usage` (missing/null => 0; output_tokens NOT counted) — src/Status/StatusStore.cs:187-190
- [x] STA-022 Context percent = integer `used*100/size` (truncation; can exceed 100) — src/Status/StatusStore.cs:200
- [x] STA-023 Model = `model.display_name` (string) else null — src/Status/StatusStore.cs:196
- [x] STA-024 Cwd = `cwd` string — src/Status/StatusStore.cs:197
- [x] STA-025 Effort = `effort.level` if non-empty, else fallback(): env `CLAUDE_CODE_EFFORT_LEVEL`, then `effortLevel` in Claude settings.json, else `"medium"` (first non-whitespace wins) — src/Status/StatusStore.cs:201,211-226
- [x] STA-026 Limits from `rate_limits.five_hour` / `rate_limits.seven_day`: `used_percentage` number (floored to int); absent => null — src/Status/StatusStore.cs:202-203,228-243
- [x] STA-027 `resets_at`: number epoch seconds (>0, int64) or ISO string (invariant, assume UTC); anything else => no reset time — src/Status/StatusStore.cs:233-242
- [x] STA-028 Updated = file's last-write time UTC — src/Status/StatusStore.cs:155
- [x] STA-029 Store lookup by session id is case-insensitive — src/Status/StatusStore.cs:36
- [x] STA-030 Unreadable/partial file keeps last good status; identical status doesn't fire Changed — src/Status/StatusStore.cs:144-168
- [x] STA-031 File read with share ReadWrite|Delete so relay's rename isn't blocked — src/Status/StatusStore.cs:150-152
- [x] STA-032 Start: creates dir, watches `*.json` (Created/Changed/Renamed, LastWrite|FileName), prunes files older than 7 days, loads all existing — src/Status/StatusStore.cs:110-127
- [x] STA-033 Prune deletes `*.json` with last-write < now-7d (errors ignored) — src/Status/StatusStore.cs:130-141
- [x] STA-034 Account limits = newest (by Updated) session that has any limit; sessions with no limits never blank them — src/Status/StatusStore.cs:60-78
- [x] STA-035 Usage-cache fallback: `%TEMP%\claude\statusline-usage-cache.json` (user's statusline.ps1 cache; read-only), keys `five_hour`/`seven_day` with `utilization`, `resets_at`; Updated = file mtime — src/Status/StatusStore.cs:84-107
- [x] STA-036 Cache wins only if newer than Claude's newest report (Claude wins on tie: `fromClaude.Updated >= cached.Updated`) — src/Status/StatusStore.cs:67
- [x] STA-037 Codex panes: status/limits come from Codex transcript (CodexSessionStore), limits = StatusStore.Newest over Codex sessions — src/MainWindow.xaml.cs:1577-1586
- [x] STA-038 Footer shows open pane's agent's limits; with no pane, Codex's if Claude not in use — src/MainWindow.xaml.cs:1564-1574

#### Git
- [x] STA-039 Branch via `git rev-parse --abbrev-ref HEAD` (trimmed, empty => null) — src/Status/GitInfo.cs:56
- [x] STA-040 Counts via `git diff --numstat --no-ext-diff --no-textconv` (unstaged vs index only; staged not counted), sum col1/col2, `-` (binary) contributes 0 — src/Status/GitInfo.cs:57,80-91
- [x] STA-041 Every git call prefixed `-c core.fsmonitor=false --no-optional-locks -C <folder>`, no window, stdout+stderr drained, 5 s timeout => kill tree, null; nonzero exit => null; git missing => null — src/Status/GitInfo.cs:94-124
- [x] STA-042 Repo detection walks up the tree for `.git` dir OR file (no process if not in a repo) — src/Status/GitInfo.cs:65-77
- [x] STA-043 Cache: per folder (case-insensitive), at most one read per 3 s interval, at most one in flight; read on thread pool; Changed fires only if status differs — src/Status/GitInfo.cs:14-50, src/MainWindow.xaml.cs:259
- [x] STA-044 Git poll every 3 s for all open panes' working dirs, only while window visible; also on pane activation — src/MainWindow.xaml.cs:261,353-357,1325

#### Strip composition (header)
- [x] STA-045 No strip when no status yet, header off, or every field empty/unticked (never zeros) — src/Status/StatusStrip.cs:38,51
- [x] STA-046 Order: Model, Branch(+diff), Context, Effort, 5h, 7d — src/Status/StatusStrip.cs:22,42-49
- [x] STA-047 Model shown if StatusModel && model present, via ModelName: `"Opus 5.5 (1M context)"` -> `"Opus 5.5 1M"` (regex `\s*\((\d+\.?\d*[kKmM])\s+context\)` -> ` $1`, trimmed) — src/Status/StatusStrip.cs:43,128-129
- [x] STA-048 Branch shown if StatusBranch and folder is a repo — src/Status/StatusStrip.cs:44
- [x] STA-049 Context meter: label `ctx`, detail `"{used}/{size}"`, tip `"Context: {used} of {size} tokens used ({pct}%)"`, no Reset — src/Status/StatusStrip.cs:54-59
- [x] STA-050 Effort shown if StatusEffort and non-empty (Codex default effort => absent) — src/Status/StatusStrip.cs:47
- [x] STA-051 5h/7d in header ONLY when footer is off (StatusFooter=false) and their tick is on and limit reported — src/Status/StatusStrip.cs:41,48-49
- [x] STA-052 Tokens: <1000 => plain int; >=1000 => `(n/1000.0)` F0 + "k" (so 999_999 -> "1000k"); >=1M => round(n/1e6, 1) (.NET Math.Round = banker's), F0 if within 0.05 of integer else F1, + "M" — src/Status/StatusStrip.cs:116-125
- [x] STA-053 ResetIn: `<=0` -> "now"; days>0 -> "{d}d{h}h" (h unpadded); hours>0 -> "{h}h{mm}m" (minutes 2-digit); else "{m}m" (truncated, so <1 min = "0m") — src/Status/StatusStrip.cs:132-139
- [x] STA-054 Bands: >=90 Red, >=70 Orange, >=50 Yellow, else Green — src/Status/StatusStrip.cs:96-102
- [x] STA-055 Meter.Band = Red if Reserve else BandFor(percent) — src/Status/StatusStrip.cs:18
- [x] STA-056 EffortStep: low 0, medium/med 1, high 2, xhigh 3, max 4, else -1 (case-sensitive) — src/Status/StatusStrip.cs:105-113
- [x] STA-057 LimitMeter labels: "5h"/"7d"; names "5-hour"/"7-day" — src/Status/StatusStrip.cs:74
- [x] STA-058 Stale (ResetsAt <= now; null reset never stale): Detail null, Reset null, tip `"{name} limit: reset since the last report, which said {p}%"`, Stale=true, never Reserve — src/Status/StatusStrip.cs:77-79
- [x] STA-059 Current limit: Detail `"↻ {reset}"` (U+21BB + space) or null without reset time; tip `"{name} limit: {p}% used"` + `", resets in {reset}"` + `". Past your {R}% reserve"` when past; Reset = bare countdown — src/Status/StatusStrip.cs:81-85
- [x] STA-060 ReserveFor: ReserveAt if that limit's reserve tick on, else 0 — src/Status/StatusStrip.cs:89-90
- [x] STA-061 PastReserve: reserveAt>0 && percent>=reserveAt && not stale — src/Status/StatusStrip.cs:93-94
- [x] STA-062 Footer: empty if footer off or no limits; 5h then 7d, each if ticked and reported; header switch irrelevant — src/Status/StatusStrip.cs:62-69
- [x] STA-063 ReserveWatch: warns once per (limit kind, ResetsAt) window; set never cleared; returns crossed meters (5h before 7d) — src/Status/StatusStrip.cs:146-161
- [x] STA-064 Reserve crossing -> island notification `IslandNote(label, "{label} at {p}%", NoteKind.Reserve)`; only a warning, prompts never blocked — src/Island/IslandWindow.xaml.cs:114, src/MainWindow.xaml.cs:1588-1595
- [x] STA-065 WarnReserve runs on status change, settings change, and every 30 s clock tick even when hidden (uses Claude limits only: `_status.Limits`) — src/MainWindow.xaml.cs:335-339,358-363,1594
- [x] STA-066 Strip/footer redraw every 30 s while visible (countdowns, staleness) and on status/git/settings change — src/MainWindow.xaml.cs:263,335-363

#### Strip drawing (themes Numbers/Bars/Rings/Chips)
- [x] STA-067 Hidden (collapsed) when strip null; clips instead of squeezing; tabular numerals — src/Status/StatusStripView.cs:21-29,32-38
- [x] STA-068 Leading 1px x 14px rule (StripRule #2E353F), margin 2/0/12/0, in every theme — src/Status/StatusStripView.cs:41
- [x] STA-069 Model field: TextPrimary text, tip "Model" — src/Status/StatusStripView.cs:43-44
- [x] STA-070 Branch field: 12px branch glyph (16-unit path, 1.3 stroke, MutedAA) + branch (TextPrimary, max 190 px, ellipsis) + `+{a}` (DiffAdded #7CC596) and `−{d}` (U+2212, DiffDeleted #E08480) only if a+d>0; tip "Branch and uncommitted diff" — src/Status/StatusStripView.cs:45-55,125-144
- [x] STA-071 Meter field Numbers: `label [detail] pct%` (label/detail MutedAA) — src/Status/StatusStripView.cs:71,74
- [x] STA-072 Meter field Bars: `"Context"|label`, 52px bar, pct — src/Status/StatusStripView.cs:65,68
- [x] STA-073 Meter field Rings: 16px ring stroke 2.5, `"Context"|label`, pct — src/Status/StatusStripView.cs:69
- [x] STA-074 Meter field Chips: `ctx|5h|7d` label, pct, then bare reset (if any) — src/Status/StatusStripView.cs:70,75
- [x] STA-075 Meter field tooltip/automation name = Meter.Tip; stale opacity 0.45 — src/Status/StatusStripView.cs:76, 304,355-360
- [x] STA-076 Effort field: Chips `"effort" value`; Bars/Rings with known step: 5 rising bars + value; else value only; tip `"Effort: {e}"` — src/Status/StatusStripView.cs:79-90
- [x] STA-077 Steps: 5 rects 3px wide, heights 4,6,8,10,12, 2px gap, bottom aligned, StepOn #A9B0BB for i<=step else StepOff #39404B — src/Status/StatusStripView.cs:402-415
- [x] STA-078 Field spacing: parts 6px apart; between fields Numbers adds 1x12 rule with 14px margins each side; Bars/Rings 14px left margin; Chips 6px — src/Status/StatusStripView.cs:97-112
- [x] STA-079 Chip: height 22, padding 9/0, radius 11, 1px border; neutral border StripRule + bg ChipBg #1F242C; meter chips tinted band color alpha .34 border / .10 fill; chip text size 11.5 (else 12) — src/Status/StatusStripView.cs:421-435
- [x] STA-080 Percent text: `"{p}%"`, SemiBold, band color — src/Status/StatusStripView.cs:326-331
- [x] STA-081 Band colors: Green #6BBF8A, Yellow #D9BE5C, Orange #E58C4E, Red #EC726C; MeterTrack #2A303A; MutedAA #8C94A1; TextPrimary #C8CDD6 — src/App.xaml:291-303,16,20
- [x] STA-082 Bar: 4px high, radius 2, track MeterTrack, fill = clamp(p,0,100)% in band color — src/Status/StatusStripView.cs:363-374
- [x] STA-083 Ring: track ellipse MeterTrack; arc from top clockwise, angle min(p/100*2π, 2π-0.001), round caps, none at 0% — src/Status/StatusStripView.cs:377-399
- [x] STA-084 Footer (LimitsView) collapsed when no meters; automation name "Account limits" — src/Status/StatusStripView.cs:153-166
- [x] STA-085 Footer Numbers: two equal columns; cell `label pct% [↻ reset]` (detail size 11.5); second column ruled off by 1px LimitRule #262C35 + 12px padding, 12px margin — src/Status/StatusStripView.cs:200-209,218-223
- [x] STA-086 Footer Bars: stacked rows 8px apart; grid cols 44 | * | 44 | 58, 10px gaps: "5-hour"/"7-day", bar, pct (right), bare reset (right, 11.5) — src/Status/StatusStripView.cs:168,189-191,226-245
- [x] STA-087 Footer Rings: two columns 10px gap; 34px ring stroke 3 with number (10.5 SemiBold, band color) centered; text "{5-hour|7-day} limit" + "resets in {reset}" (11, MutedAA) — src/Status/StatusStripView.cs:208,248-268
- [x] STA-088 Footer Chips: two columns 8px gap; card radius 8, pad 10/7, border band alpha .30, bg band alpha .08; long name (11.5) left + pct (15) right; "resets in {reset}" below — src/Status/StatusStripView.cs:208,271-297

#### Settings page (SettingsPane)
- [x] STA-089 Header "Settings" with close × (tooltip "Close (Esc)", name "Close settings") -> back to previous session — src/Settings/SettingsPane.xaml:204-217, .cs:572
- [x] STA-090 Nav (188px column): General, Status bar, Security, ─, Agents (opens setup window; nav check snaps back to visible page), Island — src/Settings/SettingsPane.xaml:227-238, .cs:555-570
- [x] STA-091 Refresh on every open: all controls from settings; cards closed (open if only 1 group — currently 1, so Publishing starts open); Your own rules closed; Codex confirm hidden — src/Settings/SettingsPane.xaml.cs:63-101
- [x] STA-092 Any change saves immediately (`SettingsChanged` -> Save + SyncCodex + redraw + WarnReserve) — src/MainWindow.xaml.cs:318-332
- [x] STA-093 Clicking anywhere on a switch row toggles the switch (unless disabled or pointer on switch) — src/Settings/SettingsPane.xaml.cs:183-188
- [x] STA-094 Switch state label "On"/"Off" beside every switch — src/Settings/SettingsPane.xaml.cs:103,146-147,156,197-198,328
- [x] STA-095 General › Start at login (not in settings.json; HKCU Run key via LoginStartup); disabled in dev builds with extra text "Only the installed Clayo can change this."; text "Clayo starts hidden; the island greets you." — src/Settings/SettingsPane.xaml:253-266, .cs:92-109
- [x] STA-096 General › Close button (info only): "Closing the window keeps sessions running. Quit Clayo from the gear menu or the island to end them." — src/Settings/SettingsPane.xaml:267-276
- [x] STA-097 General › Updates: title "Clayo {x.y.z}"; link "What's new in this version" (opens release URL) when no newer; else "Clayo {v} is out." + Update button; while waiting "Clayo {v} installs as soon as no session is working." + button "Call off" — src/Settings/SettingsPane.xaml.cs:164-178
- [x] STA-098 General › Check for updates switch (updateCheck, default on): "Once at start and every 12 hours, from GitHub. Updating is always your click." Turning on checks at once; off forgets earlier finding — src/Settings/SettingsPane.xaml:299-313, src/MainWindow.xaml.cs:321-323
- [x] STA-099 General › Coming later: "Island — Where it sits, what it announces, how long notices stay." with "Later" tag, dashed rim — src/Settings/SettingsPane.xaml:317-335
- [x] STA-100 Status bar intro: "Live session info drawn by Clayo itself, not the terminal. Colours follow how full each limit is." — src/Settings/SettingsPane.xaml:341-342
- [x] STA-101 Status bar › Theme segmented: Numbers | Bars | Rings | Chips (default Numbers) — "How the numbers are drawn, in the pane header and the sidebar footer." — src/Settings/SettingsPane.xaml:349-355, .cs:37-38
- [x] STA-102 Where it shows › "Show in the pane header" (statusHeader, default on): "Model, branch, context and effort beside the session title." — src/Settings/SettingsPane.xaml:364-376
- [x] STA-103 Where it shows › "Show limits in the sidebar footer" (statusFooter, default on): "The 5-hour and 7-day limits are account-wide, so they sit above New session. Off moves them into the pane header." — src/Settings/SettingsPane.xaml:377-391
- [x] STA-104 Fields › Per session ticks: Model (statusModel), Branch and diff (statusBranch), Context used (statusContext), Effort (statusEffort), all default on; disabled (kept ticked) when header off with note "Pane header is off" — src/Settings/SettingsPane.xaml:411-418, .cs:199,201
- [x] STA-105 Fields › Account ticks: 5-hour limit (statusFiveHour), 7-day limit (statusSevenDay), default on; disabled when header AND footer off; note "Both placements are off" / when only footer off "Shown in the pane header while the footer is off" — src/Settings/SettingsPane.xaml:419-424, .cs:200,202-204
- [x] STA-106 Reserve › "Warn near a limit" segmented Off | 70% | 80% | 90% (reserveAt, default 0=Off): "Past this much of a limit, the island says so once and the limit turns red. Prompts still go through." — src/Settings/SettingsPane.xaml:432-443, .cs:39-40
- [x] STA-107 Reserve ticks 5-hour limit (reserveFiveHour) / 7-day limit (reserveSevenDay), default on; disabled when reserve Off — src/Settings/SettingsPane.xaml:451-452, .cs:206
- [x] STA-108 Segment style: radius 6, border Hairline, checked bg #24D27556 + border Clay #D27556 + TextPrimary text — src/Settings/SettingsPane.xaml:146-172
- [x] STA-109 Security intro: "What sessions Clayo starts have to ask you before they do it." — src/Settings/SettingsPane.xaml:463-464
- [x] STA-110 Security agents box (shown only if useCodex): CC row "Claude Code — Only sessions Clayo starts." (only if useClaude); CX row switch "Also apply to Codex" (alsoForCodex, default off), text (BandYellow) "Applies to Codex outside Clayo too." + " Turning it on asks you once." when off — src/Settings/SettingsPane.xaml:467-496, .cs:323-332
- [~] STA-111 Turning Codex on shows modal "Also ask in Codex?" / "Codex can't keep this inside Clayo. It will also ask in Codex sessions you start outside Clayo. Turn it off here any time." Buttons Cancel (focused, Esc) / "Turn on for Codex"; turning off needs no confirm — src/Settings/SettingsPane.xaml:576-593, .cs:391-442 — not ported per D1 (the switch is locked; nothing to confirm)
- [x] STA-112 "Ask before" cards: one per group; header icon, title, line, state text ("Off" | "On" | "On · {n} of {total}"), switch, chevron (E70E open / E70D closed; name "Hide|Show {Title} rules"); header click toggles open — src/Settings/SettingsPane.xaml.cs:219-265,314-318,334-347
- [x] STA-113 Group switch writes askGroupsOn; group off keeps its rule list visible but disabled — src/Settings/SettingsPane.xaml.cs:345,384-389
- [x] STA-114 Rule row: tick (Label, help = Line), mono 11px line text, CC/CX chips; api row line: codex-only => "every gh api call"; Codex gets it => "{Line} · Codex: every gh api call" — src/Settings/SettingsPane.xaml.cs:267-294,356-359
- [x] STA-115 Chips: CC visible if useClaude, CX if useCodex; opacity 1 if active else 0.4; line opacity 0.6 when group off — src/Settings/SettingsPane.xaml.cs:360-364
- [x] STA-116 CC tooltip "Claude: asks first"/"Claude: runs without asking"; CX tooltip "Codex: runs without asking" | "Codex: Also apply to Codex is off" | "Codex: asks before every gh api call" (api) | "Codex: asks first" — src/Settings/SettingsPane.xaml.cs:365-369
- [x] STA-117 Ask footer text: all groups off "Off. Only your own rules apply."; codex-only & AlsoForCodex off "Codex asks once Also apply to Codex is on. Your own rules still apply."; else "Applies to sessions you start from now on. Your own rules still apply." — src/Settings/SettingsPane.xaml.cs:372-374
- [x] STA-118 "Your own rules" (read-only tag) collapsible; closed header counts "Claude {n} · Codex {m}" or "None"; per agent section CC "Claude Code"/CX "Codex" with "Open file" link (#E39A7E), problem line, rows — src/Settings/SettingsPane.xaml:506-527, .cs:459-505
- [x] STA-119 Own rule row: decision pill (allow #9FD3B1 on #246BBF8A; deny/forbidden #EC9C99 on #29D2605F; ask/prompt/other #9CCBEF on #295AA9E6), mono rule text with tooltip, note in NeedsInput #5AA9E6 — src/Settings/SettingsPane.xaml.cs:507-546
- [x] STA-120 Open file: shell-open; on failure `explorer.exe /select,"path"` — src/Settings/SettingsPane.xaml.cs:549-553
- [x] STA-121 Island page: "Send a screenshot with Ctrl+Alt+S" (screenshotHotkey, default on), text normally "Take a screenshot (Win+Shift+S), then press Ctrl+Alt+S: the island asks which session it is for."; if hotkey taken "Another app has Ctrl+Alt+S, so the shortcut is off. Close that app's shortcut, then switch this off and on." — src/Settings/SettingsPane.xaml:540-552, .cs:144-151
- [x] STA-122 Island page: "Offer to send every new screenshot" (screenshotOffer, default off): "The island asks each time you take one with Win+Shift+S, without the shortcut. Not while Clayo is in front." — src/Settings/SettingsPane.xaml:553-567
- [x] STA-123 Card/Ask state is not persisted (cards open state reset each open) — src/Settings/SettingsPane.xaml.cs:215-217

#### Settings model
- [x] STA-124 Load never fails: missing/unreadable/broken/non-object file => all defaults; per key fallback on wrong type; unknown keys ignored — src/Settings/ClayoSettings.cs:73-135
- [x] STA-125 reserveAt accepted only if int32 in {0,70,80,90}, else 0 — src/Settings/ClayoSettings.cs:53,118-119
- [x] STA-126 Enums (theme, defaultAgent) parsed by name case-insensitively, numbers/unknown => default — src/Settings/ClayoSettings.cs:104-107
- [x] STA-127 Paths: empty string or non-string => null — src/Settings/ClayoSettings.cs:90-93
- [x] STA-128 Lists (askGroupsOn, askOff): array of non-empty strings without commas, distinct, ordinal sorted, kept internally as comma string; non-array => default "" — src/Settings/ClayoSettings.cs:96-101
- [x] STA-129 NewSessionKind: !useCodex => Claude; !useClaude => Codex; else defaultAgent (Ask => null => ask each time) — src/Settings/ClayoSettings.cs:56-64
- [x] STA-130 Save: write `settings.json.tmp` indented then rename over; IO errors swallowed — src/Settings/ClayoSettings.cs:138-183
- [x] STA-131 Start at login is NOT in settings.json (Run key) — src/Settings/ClayoSettings.cs:13-14

#### AskRules / Codex
- [x] STA-132 Group "publishing" "Publishing" "Pushing, tags, releases, anything that changes GitHub." icon U+EA18 — src/Settings/AskRules.cs:25
- [x] STA-133 Rules (id / label / line / Claude patterns / Codex prefixes): push "Push code" "git push" [`git push*`,`git -C * push*`] [git push]; tags "Tags" "git tag: making or moving a version tag" [`git tag*`,`git -C * tag*`] [git tag]; releases "Releases" "gh release: drafting or publishing one" [`gh release*`] [gh release]; merge "Merge pull requests" "gh pr merge" [`gh pr merge*`] [gh pr merge]; repo "Repo settings" "Renaming, deleting, rulesets" [`gh repo edit*`,`gh repo delete*`,`gh repo rename*`,`gh ruleset*`] [gh repo edit][gh repo delete][gh repo rename][gh ruleset]; secrets "Secrets" "gh secret" [`gh secret*`] [gh secret]; workflows "Workflows" "gh workflow: running or turning one on or off" [`gh workflow*`] [gh workflow]; api "Other GitHub changes" "gh api calls that write" [`gh api *-X*`,`gh api *--method*`,`gh api *-f *`,`gh api *-F *`,`gh api *--field*`,`gh api *--raw-field*`,`gh api *--input*`] [gh api] — src/Settings/AskRules.cs:27-46
- [x] STA-134 Default: every group off (askGroupsOn ""), every rule ticked (askOff ""); new groups arrive off, new rules arrive ticked — src/Settings/ClayoSettings.cs:44-50
- [x] STA-135 ClaudeAsk = for each ticked rule of each on group, each pattern p => `Bash(p)` then `PowerShell(p)` (interleaved per pattern) — src/Settings/AskRules.cs:57-58
- [~] STA-136 Codex `clayo.rules` at `<CODEX_HOME or ~/.codex>\rules\clayo.rules`; exists only if alsoForCodex && ≥1 ticked rule in an on group; written/rewritten (only if differs)/deleted on every settings change and at startup; Codex home not created if missing — src/Settings/AskRules.cs:60-106, src/MainWindow.xaml.cs:313,326,581 (D1)
- [x] STA-137 Settings › General intro "How Clayo starts and what the window's close button does." — Settings/SettingsPane.xaml:248
- [x] STA-138 Settings › Island page heading "Screenshots" and intro "Send a screenshot to a session: the island asks which one, and the image lands in its prompt, ready for your question." — Settings/SettingsPane.xaml:534-536
- [x] STA-139 Settings section headings "Coming later", "Where it shows", "Per session", "Ask before" — Settings/SettingsPane.xaml:317,360,412,498
- [x] STA-140 (D1) When alsoForCodex is true and `<CODEX_HOME>\rules\clayo.rules` exists, Security shows "Still active in Codex; delete this file to turn it off" + Open file link; the Rust build never writes or deletes it — PORTING.md §6 D1

### 3. DATA FORMATS

#### `%LOCALAPPDATA%\Clayo\settings.json` (UTF-8, no BOM, Utf8JsonWriter Indented = 2-space, camelCase, hand-written order)
| key | type | default | notes |
|---|---|---|---|
| statusHeader | bool | true | |
| statusFooter | bool | true | |
| statusModel | bool | true | |
| statusBranch | bool | true | |
| statusContext | bool | true | |
| statusEffort | bool | true | |
| statusFiveHour | bool | true | |
| statusSevenDay | bool | true | |
| reserveAt | int | 0 | one of 0,70,80,90; other/wrong type => 0 |
| reserveFiveHour | bool | true | |
| reserveSevenDay | bool | true | |
| theme | string | "numbers" | written lowercase: numbers/bars/rings/chips; read case-insensitive by name only |
| setupDone | bool | false | setup window shows until true |
| useClaude | bool | true | |
| useCodex | bool | true | |
| defaultAgent | string | "claude" | lowercase: claude/codex/ask |
| claudePath | string? | absent | written only if non-null; "" => null |
| codexPath | string? | absent | same |
| screenshotHotkey | bool | true | |
| screenshotOffer | bool | false | |
| updateCheck | bool | true | |
| askGroupsOn | string[] | [] | group ids switched ON |
| askOff | string[] | [] | rule ids unticked |
| alsoForCodex | bool | false | |
Write: `settings.json.tmp` then rename (overwrite). Rust: must write exactly these keys (preserve order for diffs) and read leniently per key (don't use strict serde derive; use `serde_json::Value`).

#### Other files in `%LOCALAPPDATA%\Clayo` (this area)
- `status\<guid>.json`: raw Claude statusline stdin bytes, one per session, atomic via `status\<guid>.<pid>.tmp`; pruned after 7 days of no writes.
- `statusline.json` (`statusline.dev.json` for dev builds): `--settings` file, compact JSON (`ToJsonString()` no indent):
  `{"statusLine":{<user keys...>,"type":"command","command":"'C:/path/clayo.exe' --statusline --pass | {\n<their cmd>\n}"},"permissions":{"ask":["Bash(git push*)","PowerShell(git push*)",...]}}`. Key order: user's keys keep position; `type` and `command` overwritten in place if present, else appended.
- (Elsewhere, not owned by Clayo dir) `%TEMP%\claude\statusline-usage-cache.json` read-only: `{"five_hour":{"utilization":2.0,"resets_at":"2026-10-03T13:39:59.976874+00:00"},"seven_day":{...}}`.
- `<CODEX_HOME>\rules\clayo.rules` (written by Clayo, ASCII only, `\n` endings):
  ```
  # Written by Clayo: Ask before publishing. Turn it off in Clayo, Settings > Security.
  # Clayo rewrites this file when those settings change, and deletes it when they're off.
  prefix_rule(pattern=["git", "push"], decision="prompt")
  ...
  ```
  (trailing newline). NOTE: this writes into ~/.codex — the plan's "never write ~/.codex" invariant conflicts; current C# does write this one file only when user opts in with confirm (D9). Flag for user decision.

#### Relay IPC
- No pipes/sockets: purely file-based. Relay process writes `status\<guid>.json`; app watches the dir (FileSystemWatcher -> `notify`), loads on Created/Changed/Renamed of `*.json`.

#### Claude statusline stdin JSON fields read
`session_id` (GUID string, required), `cwd`, `model.display_name`, `effort.level`, `context_window.context_window_size`, `context_window.current_usage.{input_tokens, cache_creation_input_tokens, cache_read_input_tokens}` (current_usage may be null), `rate_limits.five_hour.{used_percentage, resets_at}`, `rate_limits.seven_day.{...}` (resets_at epoch seconds number or ISO string). Ignored: transcript_path, workspace, version, output_style, cost, totals, used_percentage of context, exceeds_200k_tokens, fast_mode, thinking, model.id.
Fallback inputs: env `CLAUDE_CODE_EFFORT_LEVEL`; `<ClaudeConfigDir>\settings.json` key `effortLevel` (string) and `statusLine` (object), `permissions.{allow,ask,deny}` (OwnRules).

### 4. HARD POINTS

#### Status line relay (end-to-end)
1. Injection: never touches `~/.claude`. On every Claude pane launch (new `--session-id`, `--resume`, fork `--resume X --fork-session --session-id Y`), ClaudeAgent.Command calls `StatusRelay.WriteSettings(Environment.ProcessPath, AskRules.ClaudeAsk(settings))`, which reads `<ClaudeConfigDir>\settings.json`'s `statusLine` fresh, builds the JSON, writes `%LOCALAPPDATA%\Clayo\statusline.json` if changed, and the command becomes `& '<claude.exe>' --session-id '<id>' --settings '<...\statusline.json>'` (PowerShell). Claude Code merges `--settings` over user settings: our `statusLine` replaces theirs for this process; `permissions.ask` arrays merge (additive; ask beats allow). Not env-based. One shared file for all panes (last write wins; content is the same function of current state).
2. Command string (Claude runs statusLine via Git Bash on Windows): exe path with backslashes -> `/`, wrapped in single quotes with `'` -> `'\''`. With user command C: `'<exe>' --statusline --pass | {\nC\n}`. Braces group their whole command (handles `cd x && ./line.sh`); the newlines stop a trailing `# comment` in C from eating `}`. Pipe means the shell starts both processes concurrently, so their startup overlaps ours.
3. Relay process (`clayo.exe --statusline [--pass]`): must be the very first check in main, before any UI/single-instance/mutex/logging — it runs on every status refresh. Reads stdin fully into memory; if `--pass`, writes the exact same bytes to stdout first (user command waits on it), ignoring broken-pipe errors; then saves the file; exit 0. No stdout output without `--pass` => Claude's in-pane status line is empty when the user had none. The user's command's stdout goes to Claude unchanged (we're upstream of it in the pipe; our stdout is their stdin). No timeouts in relay itself (Claude Code owns status line timeouts); relay must be fast. Rust: `std::io::stdin().read_to_end`, `stdout().write_all` + flush, ignore `ErrorKind::BrokenPipe`. Keep binary tiny-startup path (no egui init; on Windows a GUI-subsystem exe still gets inherited pipe handles — C# app is WinExe? verify subsystem: if `windows_subsystem="windows"`, std handles from pipes still work when spawned with redirected handles).
4. Save: parse JSON -> `session_id` must be GUID -> write `<guid>.<pid>.tmp` then rename over `<guid>.json`. Malformed => skip silently. Never fail the user's line.
5. App side: watcher on `status\*.json` + initial scan + 7-day prune. Reader opens with share-delete so rename isn't blocked (Rust: `std::fs::File::open` on Windows uses FILE_SHARE_READ|WRITE|DELETE by default — OK). Partial reads ignored (keep last good).
6. Limits: newest session report with limits, unless the user's statusline.ps1 cache file is strictly newer.
7. DEBUG builds use `statusline.dev.json` so a dev exe relays through itself. Rust: `cfg(debug_assertions)`.

#### AskRules (Ask before publishing) semantics
- Claude: per-pane `permissions.ask` via `--settings` (Clayo's sessions only). `ask` overrides user `allow` (D3/D8). Both `Bash(...)` and `PowerShell(...)` forms per pattern.
- Codex: global `~/.codex/rules/clayo.rules` with `prefix_rule(..., decision="prompt")` (applies outside Clayo too, hence confirm modal). Codex matches prefixes only, so `api` rule becomes every `gh api` call (D6).
- Settings: `askGroupsOn` (opt-in groups) / `askOff` (opt-out rules) so new groups default off and new rules default ticked (D4). Unknown ids ignored.
#### OwnRules (read-only)
- Claude: `<ClaudeConfigDir>\settings.json` `permissions.allow|ask|deny` string arrays, in that order; non-array/non-string/empty skipped. Missing file => Problem "No settings file yet." (File null); parse/IO error => "Couldn't read this file." (File = path). Note only on allow rules exactly equal to `Bash(p)`/`PowerShell(p)` for a known AskRule pattern: "{Label} still asks first" if that rule is currently ticked in an on group, else "{Label} runs without asking".
- Codex: all `*.rules` in `<CODEX_HOME>\rules` except `clayo.rules` (case-insensitive), sorted ordinal-ignore-case. None => "No rules files yet.". Regex `prefix_rule\s*\(([^)]*)\)` (singleline), inside: `pattern\s*=\s*\[([^\]]*)\]` (skip if absent), `decision\s*=\s*"(\w+)"` (default "allow"), words `"([^"]*)"`. Shown as `["w1", "w2"]`. Note on allow when words exactly equal a known Codex prefix: asks iff alsoForCodex && ticked. IO error => File=files[0], "Couldn't read these files.". File = first file.
#### Git
- Commands and flags as in BEHAVIORS; 5 s timeout with kill; per-folder 3 s throttle; walks up for `.git`; security: `core.fsmonitor=false`, `--no-ext-diff --no-textconv`, `--no-optional-locks`. Read off UI thread; Changed only on difference. Plan allows `git2`, but behavior parity (numstat = working tree vs index, branch "HEAD" when detached from `--abbrev-ref`) is easiest with the `git` CLI; spawn with CREATE_NO_WINDOW.

### 5. CHECKS (port to cargo test)

#### checks/status.cs
Fixtures: `fresh` (Claude 2.1.288 startup payload: id 3f136615-e5ba-45b2-9766-6cc4d88ef5d9, cwd C:\work, effort medium, display_name "Opus 5.5 (1M context)", context_window_size 1000000, current_usage null, no rate_limits) and `busy` (effort xhigh, usage in 12/out 900/cc 40000/cr 300000, five_hour 42.7 @1790000000, seven_day 9 @"2026-10-08T10:00:00Z"); `at` = today UTC midnight; `Unused` fallback panics.
- fresh: session id == "3f136615-e5ba-45b2-9766-6cc4d88ef5d9" — :31
- fresh: model == "Opus 5.5 (1M context)" — :32
- fresh: cwd == `C:\work` — :33
- fresh: context size == 1_000_000 — :34
- fresh: context used == 0 (null usage) — :35
- fresh: context percent == 0 — :36
- fresh: effort == "medium" (fallback not called) — :37
- fresh: five_hour None — :38
- fresh: seven_day None — :39
- fresh: Updated == file time passed in — :40
- busy: context used == 340_012 (in+cc+cr) — :45
- busy: context percent == 34 — :46
- busy: 5h percent floored 42 — :47
- busy: 5h reset == unix 1790000000 — :48
- busy: 7d percent 9 — :49
- busy: 7d reset == 2026-10-08T10:00:00Z — :50
- Effort("high","low","max") == "high" — :54
- Effort(null,"low","max") == "low" — :55
- Effort(null,"","max") == "max" — :56
- Effort(null,null,null) == "medium" — :57
- Parse(`{"session_id":"a1b2c3d4-0000-0000-0000-000000000000"}`, fallback "low").effort == "low" — :58
- Parse(busy[..60]) == None — :62
- Parse("Claude") == None — :63
- Parse("[1,2]") == None — :64
- Parse(`{"cwd":"C:\\work"}`) == None — :65
- Parse(`{"session_id":"..\\..\\evil"}`) == None — :66
- Store over temp dir: a=busy@at, b=busy with 77@at+5m, c=fresh@at+9m, "broken.json"=busy[..80]@at+10m; usage cache path nonexistent; Start():
  - Get(a).context_percent == 34 — :94
  - Get(B uppercase).five_hour.percent == 77 — :95
  - Limits.five_hour == 77 (newest with limits; c has none) — :96
  - Get("broken") == None — :97
  - rewrite a with 51 @at+20m, Load -> Limits.five_hour == 51 — :100-101
  - write b truncated `{"session_id":"b","cwd":` @at+30m, Load -> Get(b).five_hour == 77 (keep last good) — :104-105
  - write x.tmp then rename to dddddddd-....json -> Changed fires for that id within 5 s — :108-113
- Usage cache fixture (five_hour utilization 2.0 reset "2026-10-03T13:39:59.976874+00:00", seven_day 16.0 "2026-10-07T01:59:59.976900+00:00", extra_usage):
  - ParseUsageCache: 5h percent 2 — :131
  - 5h reset == 2026-10-03 13:39:59.976874 UTC (microsecond precision) — :132
  - 7d percent 16 — :133
  - updated == passed time — :134
  - `{"extra_usage":{}}` => None — :135
  - `{"five_hour":` => None — :136
- Store with cache path in temp dir2:
  - no sessions, no cache => Limits None — :146
  - cache written mtime=at => Limits.seven_day == 16 — :148-150
  - session a (busy) mtime at+1m loaded => seven_day == 9 (Claude newer wins) — :153-157
  - cache mtime at+2m => seven_day == 16 (newer cache wins) — :158-159
- ParseNumstat("3\t1\ta.cs\n10\t0\tb.cs\n-\t-\tlogo.png\n") added == 13 — :167
- same deleted == 1 — :168
- ParseNumstat("") == (0,0) — :169
- IsRepo(temp dir) == false — :170
- Read(temp dir) == None — :171
- IsRepo(cwd inside checkout) == true — :173 (Rust: use CARGO_MANIFEST_DIR or a temp `git init`)
- Read(cwd).branch non-empty — :174

#### checks/strip.cs
Fixtures: now = 2026-10-03T12:00:00Z; all = default settings; status = (id, "Opus 5.5 (1M context)", C:\work, used 340_012, size 1_000_000, 34, "xhigh", 5h Limit(62, now+1h20m), 7d Limit(14, now+4d10h), at); git = ("feature/upgrade-clayo-v1", 12, 3); limits = (status.5h, status.7d, at).
- BandFor 0 Green, 49 Green, 50 Yellow, 69 Yellow, 70 Orange, 89 Orange, 90 Red, 104 Red — :25-32
- Tokens 999 "999", 340_012 "340k", 200_000 "200k", 1_000_000 "1M", 1_500_000 "1.5M" — :35-39
- ModelName "Opus 5.5 (1M context)"->"Opus 5.5 1M"; "Sonnet 5.5 (200k context)"->"Sonnet 5.5 200k"; "Haiku 4.5"->"Haiku 4.5" — :42-44
- ResetIn now+1h20m "1h20m"; now+3h5m "3h05m"; now+4d10h "4d10h"; now+20m30s "20m"; now-1m "now" — :47-51
- For(status,git,limits,all): model "Opus 5.5 1M"; git == git; context label "ctx"; detail "340k/1M"; percent 34; band Green; tip "Context: 340k of 1M tokens used (34%)"; effort "xhigh"; five_hour/seven_day None (footer on) — :55-63
- Footer off: 5h label "5h"; detail "\u21BB 1h20m"; band Yellow; tip "5-hour limit: 62% used, resets in 1h20m"; 7d tip "7-day limit: 14% used, resets in 4d10h" — :69-73
- Footer off + StatusSevenDay off => seven_day None — :74-75
- Footer off + limits None => five_hour None — :76-77
- LimitMeter(true, Limit(5,None), now).detail None — :78-79
- Footer(limits,all,now)[0].reset == "1h20m" — :83
- For(...).context.reset None — :84
- EffortStep low 0, medium 1, med 1, xhigh 3, max 4, "turbo" -1 — :87-92
- Footer labels "5h 7d"; [0].detail "↻ 1h20m"; [1].band Green; [0].stale false; footer off => 0; limits None => 0; StatusFiveHour off => single "7d"; header off => 2 — :96-105
- Footer at now+2h: [0].stale true; [0].tip "5-hour limit: reset since the last report, which said 62%"; [0].detail None; [1].stale false — :108-113
- Footer at now+30m [0].detail "↻ 50m" — :114
- Reserve at80, near = (Limit(82, now+1h), Limit(14, now+4d)): [0].band Red; [1].band Green; [0].tip "5-hour limit: 82% used, resets in 1h00m. Past your 80% reserve"; reserve off => [0] Orange; ReserveFiveHour off => Orange; header (footer off) 5h band Red; at now+2h (stale) [0] Orange — :119-130
- ReserveWatch: first Crossed(near,at80,now) labels "5h"; second call count 0; Crossed(None) 0; next window 5h=Limit(85, now+6h) at now+1.5h => "5h"; both (91,90) with ReserveAt 90 => "5h,7d"; reserve off => 0; stale (now+2h) => 0 — :132-141
- context 810_000 / 81% => band Orange — :144-145
- For(None,...) None; header off None; model/branch/context/effort all off => None — :148-151
- model+effort off, git None: (model,effort) None; git None; context percent 34 — :154-157
- status.model None => strip.model None — :159-160

#### checks/settings.cs (temp dir; never real file)
- Load(missing) == defaults — :20
- defaults == ClayoSettings(true x8) (all field flags true) — :21
- changed {header off, effort off, 7d off} Save then Load == changed — :23-25
- no temp file left (dir has 1 file) — :26
- `{"statusHeader":false,"someLaterSetting":{"x":1}}` => defaults+header off — :28-29
- `{"statusModel":false}` => defaults+model off — :31-32
- `{"statusModel":"no","statusBranch":false}` => defaults+branch off — :34-36
- defaults.screenshotHotkey true; screenshotOffer false — :39-40
- {hotkey false, offer true} round-trips — :41-43
- defaults.updateCheck true — :46
- `{"statusHeader":false}` => updateCheck true — :47-48
- `{"updateCheck":false}` => defaults+updateCheck false — :49-50
- save updateCheck false => loads false — :51-52
- defaults.reserveAt 0 — :55
- {reserveAt 80, reserveSevenDay false} round-trips — :56-58
- `{"reserveAt":55}` => 0 — :59-60
- `{"reserveAt":"90"}` => 0 — :61-62
- defaults.theme Numbers — :65
- Rings round-trips — :66-68
- file contains `"theme": "rings"` (indented ": " separator) — :69
- `{"theme":"voxel"}` => Numbers — :70-71
- `{"theme":"2"}` => Numbers — :72-73
- defaults.setupDone false; useClaude && useCodex true; defaultAgent Claude — :77-79
- setup {setupDone, useCodex false, defaultAgent Ask, claudePath C:\tools\claude.exe, codexPath None} round-trips — :80-86
- file lacks "codexPath" when None — :87
- NewSessionKind: default Claude; DefaultAgent Codex => Codex; Ask => None; setup (useCodex false) => Claude; useClaude false => Codex — :88-92
- file contains `"defaultAgent": "ask"` — :93
- `{"defaultAgent":"gemini","claudePath":"","codexPath":3}` => defaultAgent Claude; claudePath None; codexPath None — :94-97
- setup + header off round-trips — :98-100
- `{"statusModel":fal` => defaults — :102-103
- `[true]` => defaults — :104-105
- "" => defaults — :106-107
- file held open exclusively: Save doesn't panic; Load => defaults — :110-114

#### checks/permissions.cs (AskRules / OwnRules / SettingsJson)
- defaults: askGroupsOn ""; ClaudeAsk count 0; askOff ""; alsoForCodex false; All.len 8; 8 unique ids — :30-35
- on={askGroupsOn "publishing"}: ClaudeAsk count == sum(patterns)*2 (=36); contains "Bash(git push*)", "PowerShell(git push*)", "Bash(git -C * push*)", "Bash(gh api *-X*)"; off => 0 — :38-44
- noTags: no entry containing "git tag" or "* tag*"; count == on-4; still has Bash(git push*) — :46-49
- group ids "publishing"; All == flatten(groups); group off keeps askOff "tags"; group off => 0; back on equals unticked list; askGroupsOn "nope,publishing" == on list — :52-57
- SettingsJson(`C:\Clayo\clayo.exe`, {type command, command "pwsh line.ps1", padding 2}, ask): statusLine.command starts with "'C:/Clayo/clayo.exe' --statusline --pass"; padding 2; permissions.ask len == ask len; permissions has 1 key; input object unchanged — :60-67
- SettingsJson(exe, None, []): no permissions; command == "'C:/Clayo/clayo.exe' --statusline" — :68-70
- old file `{"statusHeader":false,"useCodex":true}`: askGroupsOn ""; askOff ""; alsoForCodex false — :74-78
- {on, askOff "push,tags", alsoForCodex} round-trips; askOff array len 2; askGroupsOn array len 1 — :80-86
- `{"askOff":["tags",3,"",null,"push","tags","a,b"]}` => "push,tags" — :88-89
- `{"askOff":"tags"}` => "" — :90-91
- `{"askGroupsOn":["nope",3,"publishing"]}` => ClaudeAsk == on list — :92-93
- CodexRules(on) None; codex rules: first line starts "# Written by Clayo"; prefix_rule lines == sum codex prefixes (=11); contains `prefix_rule(pattern=["git", "push"], decision="prompt")` and `["gh", "api"]` line; group off => None; tags unticked => no `"tag"`; all unticked => None — :96-106
- SyncCodex in temp home with user default.rules: on (codex off) => no clayo.rules; codex => file == rules text; tags off => no `"tag"`; group off => deleted; codex then on => deleted; user file untouched; rules dir has 1 file; nonexistent home not created — :115-132
- OwnRules Claude: missing => "No settings file yet."; file with allow [npm test, git push], ask [git commit], deny [Read(./.env)] => decisions "allow allow ask deny"; [3].rule "Read(./.env)"; [0].note ""; [1].note "Push code still asks first"; askOff push => "Push code runs without asking"; group off => "Push code runs without asking"; File == path; broken JSON => "Couldn't read this file."; allow not array => 0 rules — :136-150
- OwnRules Codex: no rules dir => "No rules files yet."; default.rules with 3 rules (prompt; multi-line allow git push with trailing comma; no-decision npm test) + clayo.rules synced: count 3; decisions "prompt allow allow"; [1].rule `["git", "push"]`; [2].rule `["npm", "test"]`; [1].note "Push code still asks first"; with codex off => "Push code runs without asking"; File name "default.rules" — :153-165

## Phụ lục D — Island


Paths below are relative to `D:\Clayo\Clayo\src\Clayo\` unless prefixed `checks/`. IW = Island/IslandWindow.xaml.cs, IX = Island/IslandWindow.xaml, IT = Island/IslandTrigger.cs, IG = Island/IslandGlow.cs, CM = Island/ClayoMascot.cs, SS = Island/Screenshots.cs, MW = MainWindow.xaml.cs.

### 1. MAPPING

| C# file / class | Rust crate::module | Purpose |
|---|---|---|
| IT `IslandState`, `NoteKind`, `IslandNote`, `PxRect`, `IslandInput`, `IslandTrigger` | `clayo-core::island::trigger` | Pure state machine (no Win32): dwell/peek/notify/compact/greet/drop/pick decisions from one poll's input + clock. `IslandNote.Source` is `object` compared with `Equals` -> use an enum `NoteSource { Pane(PaneId), Limit(String), Clayo(&'static str) }` (Eq+Hash). |
| SS `Screenshots.Dir/KeepFor/NewPath/Prune` | `clayo-core::screenshots` | File naming `yyyy-MM-dd_HH-mm-ss[-n].png`, 7-day prune of `*.png`. Pure fs, testable. |
| SS `OnClipboard/SaveFromClipboard` | `clayo-island::clipboard` (Win32) | Clipboard has image? / save CF_DIB as PNG (BGR, alpha dropped) with retry. Use `png` crate. |
| CM `ClayoMascot`, `MascotMove` | `clayo-island::mascot` (+ `clayo-island::anim` for keyframes/easings) | Voxel mascot in 64x64 grid, layered transforms, 7 moves, drawn with tiny-skia. |
| IG `IslandGlow` | `clayo-island::glow` | Second layered, fully click-through topmost window (760x200 logical) for 3-layer radial light + 2px dwell bar; colour/spread/breath animations. |
| IX `IslandWindow.xaml` | `clayo-island::view` | Pill layout: header (mascot, dot, two text lines), drop body, pick rows; colours/fonts/sizes. |
| IW `IslandWindow` (poll Tick, SlideIn/Out, Present, ClickThrough, Place) | `clayo-island::window` | Win32 layered NOACTIVATE TOOLWINDOW TOPMOST hwnd, UpdateLayeredWindow, 50/250 ms cursor poll, slide/width spring animations, per-monitor DPI placement. |
| IW `IsBusy/ForegroundFillsMonitor/InMoveSize/InOleDrag/PrimaryButtonDown/ClayoInFront` | `clayo-island::sys` | Win32 probes feeding `IslandInput`. |
| IW `ApplyShots/WndProc/OnHotkey/OnClipboard/StartPick/Choose/CancelPick/BuildPickRows` | `clayo-island::shots` | Ctrl+Alt+S RegisterHotKey, picker keys 1/2/3/N/Esc, AddClipboardFormatListener auto-offer. |
| IW `Pill_DragOver/DragLeave/Drop/Mention` | `clayo-island::drop` | OLE `IDropTarget` on island hwnd; folder -> new session, file -> session in its folder with `@file ` prefill. `Mention` itself is pure -> `clayo-core::island::mention`. |
| IW context menu (`Menu_Opened/Login_Click/Quit_Click`) + MW `ShowLoginState/ApplyLoginState/Quit` | `clayo-island::menu` + `clayo-app` (login startup lives in core/system) | "Start at login" (checkable) / "Quit Clayo". |
| MW events `SessionNotice`, `SessionWorking`, `ReserveCrossed`, `SettingsApplied`; methods `OpenSessionCount`, `RecentSessions`, `Reveal`, `ShowSession`, `SendImage`, `AdoptFolder`, `Quit`, `ScreenshotKeyTaken`; record `PickChoice` | `clayo-app::island_bridge` (trait `IslandHost` + channel of `IslandEvent`) | The boundary between app and island crate. |
| App.xaml.cs `_island = new IslandWindow(window); if (background) _island.Greet();` | `clayo-app::main` | Island created after main window; greet on `--background` (login) start. |
| checks/island.cs | `clayo-core/tests/island_trigger.rs` | ~110 trigger assertions. |
| checks/shots.cs | `clayo-core/tests/screenshots.rs` | NewPath / Prune. |
| checks/mascot.cs | `clayo-island/examples/mascot_render.rs` (or `#[test] #[ignore]`) | Visual: renders PNGs idle/peek at 256 and idle at 44 on #0f1114. |

### 2. BEHAVIORS

#### States, kinds, notes
- [x] ISL-001 States: Hidden, Compact, Peek, Notify, Drop, Pick — IT:9
- [x] ISL-002 Kinds: NeedsYou, Error, Done, Reserve, Tip; Urgent = NeedsYou|Error (Reserve/Tip/Done not urgent) — IT:16,24
- [x] ISL-003 PxRect is right/bottom-exclusive; Contains(x,y)= L<=x<R && T<=y<B — IT:28-31
- [x] ISL-004 Notify(note) removes any queued note with same Source then appends with ++seq (one note per session; newer replaces) — IT:161-165
- [x] ISL-005 Resolve(source) drops queued notes from that source (session moved on/closed) — IT:171
- [x] ISL-006 Note choice: urgent before non-urgent, then highest seq (newest) — IT:410-415
- [x] ISL-007 A note becomes Note when best != current; `_shownAt` = now (Done's 4 s counts from on-screen, not queue time) — IT:418-425
- [x] ISL-008 Non-urgent note (Done/Reserve/Tip) expires when now-shownAt>=4000 AND cursor not over island AND it has been away >=400 ms (hover holds) -> resolved — IT:404-408,438-441
- [x] ISL-009 Urgent notes (NeedsYou/Error) never time out; stay until Resolve or Dismiss — IT:402-432; checks/island.cs:146,154-157
- [x] ISL-010 Notes outrank Peek and Compact (Notify replaces a peek/compact) — IT:247; checks/island.cs:163,440
- [x] ISL-011 While a note shows and none remain, state Notify -> Hidden — IT:248
- [x] ISL-012 Pane status -> note kind: NeedsInput->NeedsYou, Error->Error, Done only when previous status was Working ->Done, anything else -> null (Resolve) — MW:1279-1288
- [x] ISL-013 Pane goes Working -> SetWorking(pane,true); leaves Working -> SetWorking(false) — MW:1289-1290
- [x] ISL-014 Closing a pane -> Resolve(pane) + SetWorking(pane,false) — MW:1138-1139
- [x] ISL-015 Reserve warning: Source = meter Label string, Name = "{Label} at {Percent}%", kind Reserve; newer warning for same limit replaces — IW:114
- [x] ISL-016 Tip "Ctrl+Alt+S is taken by another app" (source "ShotKey") when RegisterHotKey fails — IW:537
- [x] ISL-017 Tip "Copy a screenshot first (Win+Shift+S)" (source "Screenshots") when hotkey pressed with no image, or save failed — IW:575,584-585,642

#### Edge dwell -> Peek
- [x] ISL-018 Edge zone: cursor y in [monTop, monTop+2) physical px (EdgePx=2, NOT scaled) and |x - centreX| <= 240*scale/2 — IT:58,61,144-150
- [x] ISL-019 Monitor = MonitorFromPoint(cursor, NEAREST); scale = GetDpiForMonitor(MDT_EFFECTIVE_DPI)/96, fallback 1.0 — IW:153-159
- [x] ISL-020 Dwell 600 ms in zone -> Peek; progress = dwelt/600 for dwell bar; DwellLengthMs = 600 — IT:63,274-286
- [x] ISL-021 Dwell resets on leaving zone, on primary button down (window snap drag), on move/size mode; held time does not count — IT:268-272
- [x] ISL-022 `_mustLeaveFirst` (after Dismiss/EndPick/Release): no new dwell until cursor leaves the zone once — IT:103,220,268
- [x] ISL-023 Peek stays while cursor in zone or over island (pill rect or open context menu); hides 400 ms after leaving — IT:64,252-260; IW:200
- [x] ISL-024 DwellProgress is 0 whenever not dwelling (out of zone, button, busy, peeking) — IT:222
- [x] ISL-025 Peek shows "clayo" + "N sessions" ("1 session" when 1) where N = number of live panes — IW:346-353; MW:283

#### Busy / Clayo-in-front ("quiet hours" = Windows Focus Assist/DND, no own setting)
- [x] ISL-026 Busy = SHQueryUserNotificationState in {QUNS_RUNNING_D3D_FULL_SCREEN(3), QUNS_PRESENTATION_MODE(4), QUNS_QUIET_TIME(6)}, or QUNS_BUSY(2) only if foreground window is visible, not iconic, not class Progman/WorkerW, and its rect covers its monitor — IW:270-290
- [x] ISL-027 Busy checked at most once per second (first tick always) — IW:69,165-169
- [x] ISL-028 ClayoActive = main window visible && not minimized && GetAncestor(GetForegroundWindow(), GA_ROOTOWNER) == main hwnd (owned windows like Setup count) — IW:298-301
- [x] ISL-029 While Busy or ClayoActive: anything showing (except Pick) hides; dwell reset; notes held (not dropped) — IT:230-241
- [x] ISL-030 ClayoActive additionally drops all non-urgent queued notes and cancels a pending greeting — IT:233,236
- [x] ISL-031 Held Done gets a full 4 s once busy clears — checks/island.cs:198-205

#### Compact (self-peek while a session works)
- [x] ISL-032 Compact pill = mascot + 8px status dot only, 92 logical wide, click-through — IW:28,319,324-326; IT:4-8
- [x] ISL-033 Shows when any working session not yet "peeked" in this run of work, rate limit at most one self-peek per 180 s; a start blocked by the limit is skipped (marked peeked), not held — IT:77,347-358
- [x] ISL-034 Session stopping work clears its peeked mark (next run counts) — IT:178-186
- [x] ISL-035 Compact hides when no session works, or after 10 s — IT:80,394-397
- [x] ISL-036 Resting cursor in zone or over pill (button up, not move/size) for 600 ms opens full Peek; a rest under way holds it past 10 s — IT:83,379-391
- [x] ISL-037 Compact: dot Blue (#78A5FF) normally; Warm when greeting — IW:325
- [x] ISL-038 Compact pose Walk; light Blue (greeting: Warm) — IW:381,391

#### Login greeting
- [x] ISL-039 App started with background (login) arg -> island.Greet() — App.xaml.cs:25,67; IW:777
- [x] ISL-040 Greeting = Compact pill with Warm dot, mascot Wave, Warm glow, out 2500 ms then hides; only once — IT:89,364-374,394-395; IW:379
- [x] ISL-041 Greeting held while busy; given up if not shown within 60 s; dropped if Clayo comes to front — IT:95,224-233
- [x] ISL-042 Greeting does not consume the self-peek; resting on it opens Peek (no longer greeting) — IT:385; checks/island.cs:500-515

#### Drop (file drag)
- [x] ISL-043 Only an OLE drag counts (foreground thread's capture hwnd has class "CLIPBRDWNDCLASS"); not move/size — IW:255-262; IT:297
- [x] ISL-044 Drag zone: y in [top, top+40*scale), |x-centre| <= 480*scale/2 — IT:70-71,152-158
- [x] ISL-045 Drag dwell 250 ms (dwell bar uses DwellLengthMs=250) -> Drop, replacing any note/peek/compact (note returns after) — IT:72,295-313
- [x] ISL-046 Drop stays while dragging inside drag zone or over island; 1500 ms grace when dragged away; 400 ms when the drag ended elsewhere — IT:73,320-329
- [x] ISL-047 `_dragEndFirst` after Dismiss/Release: the same drag ignored until it ends; a new drag may bring it straight back — IT:108,221,335-340,447-453
- [x] ISL-048 Drop UI: header "clayo" / sub "drop to start"; body 110 tall: dashed rounded target "Drop here" / "A file to ask about it · a folder to open it in Clayo" — IW:339-345; IX:52-61
- [x] ISL-049 Target fill faint warm (alpha 0x0F) idle, 0x29 while a valid file drag is over — IW:732-752
- [x] ISL-050 Only the first path of a FileDrop accepted (text/browser image refused, effect None); Copy effect — IW:725-737
- [x] ISL-051 Drop of folder -> main.AdoptFolder(path) (new session there, window revealed) — IW:766; MW:511-521
- [x] ISL-052 Drop of file -> AdoptFolder(fileDir, prefill "@name " or `@"name with space" `) typed into Claude input when ready, not sent — IW:767,774
- [x] ISL-053 After drop: trigger.Release() + SlideOut — IW:763-764
- [x] ISL-054 Click on drop target does nothing (not "open Clayo") — IW:508

#### Pick (screenshot picker)
- [x] ISL-055 Ctrl+Alt+S (RegisterHotKey id 1, MOD_CONTROL|MOD_ALT|MOD_NOREPEAT, VK 'S'=0x53) registered while setting ScreenshotHotkey (default true) — IW:533-543; Settings/ClayoSettings.cs:36
- [x] ISL-056 MainWindow.ScreenshotKeyTaken = setting on && registration failed; Settings view shows it — IW:544; MW:331
- [x] ISL-057 Hotkey: ignored if already Pick; if clipboard has image -> StartPick else Tip "Copy a screenshot first (Win+Shift+S)" — IW:571-576
- [x] ISL-058 Auto offer (setting ScreenshotOffer, default false): AddClipboardFormatListener; on WM_CLIPBOARDUPDATE with new GetClipboardSequenceNumber, if not Pick, Clayo not in front, not busy, image on clipboard -> StartPick — IW:546-555,592-601
- [x] ISL-059 Hotkey overrides busy/Clayo-in-front; auto-offer does not — IW:587-591; IT:192-200
- [x] ISL-060 StartPick: choices = up to 3 open panes, most recently used first (then remaining panes in host order) — IW:605; MW:1524-1532
- [x] ISL-061 Choice row = (pane, row name or "new session", project/folder name, pane status) — MW:1530-1531
- [x] ISL-062 While Pick: global hotkeys (no modifier, MOD_NOREPEAT) 1,2,3,N,Esc registered (ids 10..14); failures silently ignored (click still works) — IW:55-56,609
- [x] ISL-063 Keys: 1..3 -> choose index if < count; N -> new session; Esc -> cancel — IW:578-581
- [x] ISL-064 Pick shows immediately (tick run synchronously) over everything, regardless of busy/ClayoActive; notes wait behind — IW:612; IT:195-200,212-217
- [x] ISL-065 Pick times out after 15 s without answer; cursor over it resets the timer — IT:86,214-215
- [x] ISL-066 After EndPick (answer/cancel), no edge peek until cursor leaves zone — IT:203-208
- [x] ISL-067 Pick keys released whenever state leaves Pick — IW:207,615-621
- [x] ISL-068 Pick header: "send screenshot" (semibold) + " to…"; sub "1–{n} or N · Esc to cancel", or "N for a new session · Esc to cancel" when n=0 — IW:333-336
- [x] ISL-069 Rows: key badge, name (semibold), "  folder" (faint), status dot right; last row "N" / "New session…" + first choice's folder, no dot — IW:649-659,674-720
- [x] ISL-070 Row hover bg #14FFFFFF; click (left up) chooses — IW:712-718
- [x] ISL-071 Choose: end pick, then save clipboard image to PNG only now (cancel leaves nothing); save fails -> Tip — IW:634-644
- [x] ISL-072 Send to existing live non-exited pane: Reveal, ShowSession, write bracketed paste `ESC[200~{path}ESC[201~` to its PTY (no Enter); Codex pane also records (path, utcNow) in CodexImages — MW:1539-1548; Terminal/TerminalPane.xaml.cs:374,377
- [x] ISL-073 New session / dead pane: AdoptFolder(folder of picked pane, else last-used pane, else current folder; fallback current folder if missing) with prefill = bracketed paste of path; Codex new pane records image — MW:1549-1553
- [x] ISL-074 Status dot colours (App.xaml:21-29): Working/Starting #D4A24C, NeedsInput #5AA9E6, Done #6BBF8A, Error #D2605F, Exited #5A6472, other #39404B — MW:164-172
- [x] ISL-075 Screenshot file: %LOCALAPPDATA%\Clayo\shots\yyyy-MM-dd_HH-mm-ss.png (local time), -2, -3... if exists — SS:15-16,50-56
- [x] ISL-076 Save converts to Bgr32 (alpha discarded, avoids fully transparent images) — SS:37-41
- [x] ISL-077 Save retries up to 5 times with 50 ms sleep on COMException (clipboard held); IO/Unauthorized -> null — SS:30-45
- [x] ISL-078 OnClipboard() returns false on COMException — SS:20-24
- [x] ISL-079 Prune at main window load: delete *.png in shots dir with LastWriteTimeUtc < now-7 days; ignores IO errors; missing dir ok — SS:18,59-68; MW:350

#### Click / Esc / menu
- [x] ISL-080 Left click (mouse up) on pill in Peek/Notify: Dismiss (resolve note), SlideOut, main.Reveal() (Show, restore if minimized, Activate; Setup first if needed), and if the note came from a pane, ShowSession(pane) — IW:504-519; MW:530-540,1301-1306
- [x] ISL-081 Pill cursor: Hand; Arrow in Drop/Pick — IW:330
- [x] ISL-082 Esc (GetAsyncKeyState edge-detected, not swallowed, still reaches foreground app) while anything shows -> Dismiss — IW:174-179
- [x] ISL-083 Dismiss: resolve showing note, hide, must leave zone first, ignore current drag — IT:447-453
- [x] ISL-084 Right-click context menu: "Start at login" (checkable; on open reads Run-key state, disabled if build can't write) / separator / "Quit Clayo" (main.Quit ends app) — IX:25-31; IW:780-784; MW:1616-1628,1602-1606
- [x] ISL-085 Open context menu counts as "over island" — IW:199-200
- [x] ISL-086 Compact and Hidden: whole window WS_EX_TRANSPARENT (clicks pass to tabs); also during slide out — IW:219-222,488-494

#### Poses and lights per state
- [x] ISL-087 Poses: Drop/Pick -> Peek; greeting -> Wave; Compact -> Walk; Peek(no note) -> Wave then Idle after 1400 ms; NeedsYou -> Alert then Talk after 1400 ms; Error/Reserve -> Alert; Done -> Jump; Tip -> Wave then Idle — IW:97-98,375-387
- [x] ISL-088 Glow colours: Compact (non-greeting) Blue; no-note (peek/drop/pick/greeting) Warm; NeedsYou Blue; Done Green; Tip Warm; Error/Reserve Amber — IW:389-397
- [x] ISL-089 Colours: Warm rgb(255,186,140), Blue (120,165,255), Amber (255,168,60), Green (60,214,160) — IG:26-29
- [x] ISL-090 Note header text: Name (semibold #ECEEF1) + rest (#8B919A): " · needs you" / " · api error" / " · past your reserve" / "" (Tip) / " · finished" (Done); sub line hidden — IW:354-367
- [x] ISL-091 After 30 s on screen (restarted on each SlideIn): mascot Still (no animation) and glow breathing stops — IW:47,99-104,372-374
- [x] ISL-092 Mascot animates only when visible, Windows client-area animations enabled (SPI_GETCLIENTAREAANIMATION) and not Still — CM:129
- [x] ISL-093 Glow breath only if client-area animations enabled — IG:161

#### Sizes / position / timing
- [x] ISL-094 Window always 420 logical wide (NoteWidth), centred on monitor top; heights: 60 normal, 190 Drop, Pick 52+34*(n+1)+12+10 — IW:26-34,405-418
- [x] ISL-095 Pill widths: Compact 92, Peek 300, Note/Drop/Pick 420 — IW:319
- [x] ISL-096 Pill: black #000000, corners bottom-left/right radius 24, top square; top-centred in the window — IX:19-20
- [x] ISL-097 Over-island hit rect = pill: centred, width pillWidth*scale, height max(60, actual pill height)*scale, recomputed every poll — IW:189-192,420-424
- [x] ISL-098 Placement in physical px via SetWindowPos(NOZORDER|NOACTIVATE) before and after Show (WM_DPICHANGED fix) — IW:426-433,496-502
- [x] ISL-099 New show from Hidden -> monitor under cursor; state/note change while visible -> stays on previously chosen monitor — IW:211-216
- [x] ISL-100 Slide in: TranslateY from current (initially -60) to 0, 450 ms BackEase(EaseOut, amplitude .3) — IW:85,435-447
- [x] ISL-101 Width: set instantly when fresh; otherwise animate to new width 400 ms same spring — IW:437-445
- [x] ISL-102 Slide out: TranslateY to -max(60, pillHeight+8), 260 ms CubicEase InOut; window Hide at end only if still Hidden — IW:463-481
- [x] ISL-103 Poll: 50 ms; 250 ms when Hidden and cursor > 200*scale px below monitor top; interval changed only on change; GetCursorPos failure (secure desktop) -> skip tick — IW:51,151,228-229
- [x] ISL-104 Pose second step 1400 ms — IW:97
- [x] ISL-105 Header: 52 tall, padding left 8 right 22; mascot 44x44 + 10 gap; texts 13 px head, 12 px sub; font "Segoe UI Variable Text, Segoe UI", Display text mode, ellipsis trimming — IX:9-10,36-47; App.xaml:31
- [x] ISL-106 Constants: DwellMs 600, LeaveMs 400, DragDwellMs 250, DragLeaveMs 1500, DoneMs 4000, SelfPeekEveryMs 180000, CompactMs 10000, RestMs 600, PickMs 15000, GreetMs 2500, GreetWithinMs 60000 — IT:63-95
- [x] ISL-107 No sounds anywhere in island code.
- [x] ISL-108 On close: stop timers, unregister pick keys, unregister Ctrl+Alt+S, remove clipboard listener, close glow — IW:133-144
- [x] ISL-109 Main window close only hides; island is the way back and holds Quit — IX:24; MW:1752-1760

### 3. DRAWING

#### Mascot (CM) — 64x64 grid units, scaled uniformly (Viewbox) to 44x44 px; aliased edges (crispEdges, no AA) — CM:91-95
Colours: body gradient top #e07f60 -> bottom #b55a3f (vertical, over each shape's own bounding box); extrusion #6e2d1b; white #ffffff; eye whites #f4f6f8; pupils #1b2126; mouth #2a0e06; glasses extrusion #050607; frame #1a1f23; lens gradient #3a4652 -> #0c1013 (bounding box of both lenses union).

Draw order (all inside group "whole" with transform Scale(about 32,52) then Rotate(about 32,52) then Translate):
1. Extrusions of each body part (each with its part's transform): copies offset (+.7,+.7),(+1.4,+1.4),(+2.1,+2.1), fill #6e2d1b. Parts:
   - legA1 rect(13,44,3,8) [legA], legB1 (20,44,3,8) [legB], legB2 (39,44,3,8) [legB], legA2 (46,44,3,8) [legA]
   - armL (2,28,9,9) [armL translate]; armR (53,28,9,9) [armR = Rotate about (53,32.5) then TranslateY]
   - body (10,12,43,33) [none]
   All extrusions drawn before all faces.
2. Faces of the same 7 parts, body gradient (gradient per rect bbox).
3. Highlight top edge rect(10,12,43,1) white α .28; left edge rect(10,12,1,33) white α .12.
4. Eye whites rect(15,23,8,6) ∪ (37,23,8,6) #f4f6f8.
5. Pupils (18,25,3,3) ∪ (40,25,3,3) #1b2126, translateX = pupils anim.
6. Mouth rect(29,37,4,1) #2a0e06, scaleY about (31,37).
7. Glasses group (translateY = glasses): bar rect(9,20,45,2); lensL poly (11,21)(28,21)(28,30)(26,32)(13,32)(11,30); lensR poly (32,21)(50,21)(50,30)(48,32)(34,32)(32,30); bridge rect(28,22,4,2).
   - extrusion of union(bar,lensL,lensR,bridge) offsets .6,1.2 fill #050607
   - bar #1a1f23; lenses lens gradient; bridge #1a1f23
   - glints white: (12,22,2,1),(44,22,2,1),(45,23,2,1),(46,24,1,1)

Static pose (not animating): every transform identity except glasses Y = 9 if Peek in moves else 0 — CM:130,145-150.

Animations (keyframes = (fraction, value), each segment eased with cubic-bezier(.45,0,.2,1); repeat forever unless noted):
- glassesY transition to 0 or 9 over 500 ms, BackEase EaseOut amplitude .45 (stands for cubic-bezier(.34,1.56,.64,1)) — CM:155-158
- Whole figure (exactly one, priority Alert > Jump > Walk > Idle-breath):
  - Alert: X 0.45 s [(0,0),(.25,-1.5),(.75,1.5),(1,0)]; rotate deg 0.45 s [(0,0),(.25,-2),(.75,2),(1,0)]
  - Jump (2 iterations then hold final=1/0): scaleX 1 s [(0,1),(.14,1.12),(.44,.95),(.72,1.09),(.86,.98),(1,1)]; scaleY [(0,1),(.14,.86),(.44,1.07),(.72,.91),(.86,1.02),(1,1)]; Y [(0,0),(.14,0),(.44,-10),(.72,0),(1,0)]
  - Walk: Y 0.5 s [(0,0),(.5,-1.2),(1,0)]; rotate 0.5 s [(0,0),(.5,1.2),(1,0)]
  - else (Idle/Talk/Wave/Peek): breathing scaleX 3.6 s [(0,1),(.5,1.018),(1,1)], scaleY [(0,1),(.5,.982),(1,1)]
- Walk (additionally): legA Y .5 s [(0,0),(.5,-2.4),(1,0)]; legB Y [(0,-2.4),(.5,0),(1,-2.4)]; armL Y [(0,0),(.5,1.2),(1,0)]; armR Y [(0,1.2),(.5,0),(1,1.2)]
- Wave: armR rotate 1.1 s [(0,-18),(.5,-60),(1,-18)] deg about (53,32.5)
- Talk: mouth scaleY .42 s [(0,1),(.4,3.2),(.7,1.8),(1,1)] about (31,37)
- Peek: pupils X 2.4 s [(0,0),(.2,0),(.35,-2),(.5,-2),(.65,2),(.8,2),(1,0)]
- Moves combine (Play(params)), but IW only ever plays one at a time.

#### Glow (IG) — window 760x200 logical, centred at top of monitor
- Three stacked rects 760x200, each = solid colour × radial alpha mask; whole stack in group with ScaleTransform (scaleX, scaleY) about (380,0) then TranslateY `drop`; group opacity `light` (0..1) inside `breath` opacity.
- Mask Falloff(peak,k,rx,ry,cy): ellipse centre (.5w, cy·h), radii (rx·w, ry·h); 33 stops r=s/32, alpha = peak·(e^(−k r²) − e^(−k))/(1 − e^(−k)); 0 outside — IG:260-276
  - aura: colour Aura(c), Falloff(.30, 2.2, .50, .80, .20)
  - halo: colour c, Falloff(.50, 3.6, .45, .55, .20)
  - core: colour Lift(c) with brush opacity `core`, Falloff(.95, 5, .27, .20, .25); core also has its own breath opacity
- Lift(c) = ((r+255)/2, (g+255)/2, (b+255)/2) integer division. Aura(c): Blue->(160,130,255), Amber->(255,120,120), Green->(70,200,235), Warm->(255,150,180) — IG:279-288
- spread s = min(1, pillWidth/420); wide = .35+.65s, tall = .55+.45s, core = .5+.5s — IW:451-453; IG:115
- Light (fresh: not lit and not fading): set colours/scale/core instantly; else colours 500 ms CubicEase InOut, scale 400 ms BackEase EaseOut .3, core opacity 400 ms CubicEase InOut. Always: drop -> 0 in 450 ms BackEase .3; light opacity -> 1 in 450 ms linear — IG:104-156
- Breath (once started, until Rest/Dim end): breath opacity 1 -> .6 over 2.8 s, auto-reverse, forever, SineEase InOut; core opacity 1 -> .7 over 3.7 s same — IG:161-165,290-296
- Dim(60): drop -> -60 and light opacity -> 0, 260 ms CubicEase InOut; at end (if not relit) stop breath, hide window unless dwelling — IG:176-196
- Dwell bar: 2 px tall rect at top centre; width animates linearly from progress·240 to 240 logical over (1−progress)·lengthMs (one animation per dwell, started on first poll that sees progress>0); disappears (width 0) the moment progress returns to 0. Fill horizontal gradient Warm α 0 @0, .9 @.3, .9 @.7, 0 @1. Bar always 240 wide even for drag dwell — IG:76-81,202-223,298-308
- Glow window shown only while lit, fading, or dwelling — IG:231-234
- Light under pill: pill (opaque black) covers the light's centre since glow sits just under the island in z-order.

#### Pill (IX/IW)
- Black rect width W, height 52 (+ body), top corners square, bottom radius 24; translateY slide.
- Drop target: rect inset 14 l/r, 14 bottom, 110 tall, radius 16, stroke 1.5 dashed [3,2]×thickness (4.5 on / 3 off px), stroke #B3FFBA8C (ARGB), fill #0FFFBA8C / #29FFBA8C on hover-drag; centred text "Drop here" 14 SemiBold #ECEEF1, 2 px gap, then 12.5 #8B919A.
- Pick body margin 10 l/r, 12 bottom; rows 34 tall, radius 10; inner margin 8 left 10 right; badge 20x20 radius 5 fill #1FFFFFFF, key text 11.5 SemiBold #ECEEF1 centred, 10 gap; name 13 SemiBold #ECEEF1 + "  folder" #8B919A, ellipsis; dot 8x8 at right, 10 gap.
- Text requires a font rasterizer (DirectWrite into a DIB, or fontdue/cosmic-text with Segoe UI Variable Text) — not provided by tiny-skia.

#### Easing formulas to reimplement
- WPF BackEase(a): easeIn f(t)=t³ − a·t·sin(πt); EaseOut = 1 − f(1−t).
- CubicEase InOut: t<.5 ? 4t³ : 1 − (−2t+2)³/2 (WPF: EaseIn t³ mirrored).
- SineEase InOut: (1 − cos(πt))/2.
- KeySpline(.45,0,.2,1): cubic-bezier, solve x->t.

### 4. HARD POINTS

- **No focus**: WPF `ShowActivated=False`, `Topmost=True`, `ShowInTaskbar=False`, `WindowStyle=None`, `AllowsTransparency=True` (=> WS_EX_LAYERED + per-pixel alpha via UpdateLayeredWindow). In OnSourceInitialized (handle created via EnsureHandle in ctor, BEFORE any Show) adds `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` to GWL_EXSTYLE — IX:6-8; IW:87-90,122-126. No WM_MOUSEACTIVATE handler (NOACTIVATE alone suffices). All moves: SetWindowPos(SWP_NOZORDER|SWP_NOACTIVATE). Rust: CreateWindowExW(WS_EX_LAYERED|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST, WS_POPUP), ShowWindow(SW_SHOWNOACTIVATE) / SWP_SHOWWINDOW|SWP_NOACTIVATE; optionally return MA_NOACTIVATE from WM_MOUSEACTIVATE as belt-and-braces.
- **Click-through**: (a) alpha-0 pixels of a ULW layered window don't hit-test, so the transparent 420 px slack never eats clicks — IX:13-18; (b) whole-window WS_EX_TRANSPARENT toggled on in Compact/Hidden (and while sliding out), off otherwise — IW:219-222,488-494; (c) glow window permanently WS_EX_TRANSPARENT|WS_EX_LAYERED|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW, so its semi-opaque light never eats clicks — IG:89-97. Cursor is detected by polling GetCursorPos, not by window messages, so it works while transparent.
- **No edge catcher window**: nothing exists at the top edge while hidden (would block a maximized browser's tab bar); 50/250 ms polling instead — IW:15-20.
- **Topmost**: static Topmost on both; glow placed with hWndInsertAfter = island hwnd (just below) or HWND_TOPMOST when dwelling without island — IG:241-253. No periodic re-assert.
- **Multi-DPI**: per-monitor-v2 physical px; compute rects with the target monitor's scale and SetWindowPos before and after Show (WPF reacts to WM_DPICHANGED by moving to suggested rect). In Rust: handle WM_DPICHANGED by ignoring the suggested rect and re-placing — IW:426-433.
- **Foreground on click**: Pill click calls main.Reveal() (Show + Activate). Allowed because the click is this process's last input event (SetForegroundWindow rules), even though the island is NOACTIVATE — IW:513-515. Same for picker key/click -> SendImage — IW:645.
- **Context menu on a NOACTIVATE window**: WPF popup works; in Win32 TrackPopupMenu normally needs SetForegroundWindow (steals focus). Recommend a custom-drawn menu inside the layered window, or TrackPopupMenuEx on a hidden owner and accept activation only on right-click. Must keep "menu open counts as over island".
- **Screenshot hotkey**: `RegisterHotKey(islandHwnd, 1, MOD_CONTROL|MOD_ALT|MOD_NOREPEAT, 0x53)` when setting on; Unregister when off/close; WM_HOTKEY (0x0312) handled in hwnd hook — IW:535,558-567. Picker keys: RegisterHotKey ids 10..14 with MOD_NOREPEAT only for VK 0x31,0x32,0x33,0x4E,0x1B while Pick shows (globally swallows 1/2/3/N/Esc system-wide for up to 15 s), unregistered when leaving Pick — IW:56,609,619.
- **Clipboard listening**: `AddClipboardFormatListener(hwnd)` when ScreenshotOffer on; WM_CLIPBOARDUPDATE (0x031D); dedupe with GetClipboardSequenceNumber (initial seq stored on enable) — IW:546-555,592-601. Image test: WPF Clipboard.ContainsImage (CF_BITMAP/CF_DIB/CF_DIBV5). Read via OpenClipboard(retry ~5×50 ms)+GetClipboardData(CF_DIB/CF_DIBV5), write PNG with alpha forced opaque.
- **Image to session**: NOT via clipboard paste. File saved to %LOCALAPPDATA%\Clayo\shots\…png; its path is written to the pane's PTY wrapped in bracketed paste `\x1b[200~PATH\x1b[201~` (no Enter, no trailing space), which Claude Code/Codex turn into an [Image #n] attachment; Codex panes also remember (path, time) for the image chip. New session gets the same string as Prefill typed after Claude's prompt is ready (1.5 s output quiet) — MW:1539-1553; Terminal/TerminalPane.xaml.cs:374-377,469-474.
- **Drop-to-open**: WPF AllowDrop on pill (OLE IDropTarget). Rust: OleInitialize on UI thread + RegisterDragDrop(islandHwnd, IDropTarget) reading CF_HDROP (DragQueryFileW index 0). Must be non-transparent in Drop state (it is). DragEnter/DragOver -> DROPEFFECT_COPY only if Drop state and a file path present, else NONE; DragLeave resets fill. Detecting the drag beforehand: GetGUIThreadInfo(0).hwndCapture class == "CLIPBRDWNDCLASS" — IW:255-262.
- **Window drag detection**: GetGUIThreadInfo(0).flags & GUI_INMOVESIZE(2); primary button via GetAsyncKeyState(VK_LBUTTON or VK_RBUTTON when SM_SWAPBUTTON) — IW:236-247.
- **Esc**: GetAsyncKeyState(VK_ESCAPE) edge-detected per poll; key NOT swallowed (no keyboard hook, no focus) — IW:171-179.
- **Busy**: SHQueryUserNotificationState + foreground-fills-monitor heuristic (see §2).
- **Rendering cost**: WPF redraws the layered windows in software every animation frame; the 30 s rest timer exists to stop that. Rust: animate only during transitions; breathing glow should be a cheap timer (~30 fps) and stopped after 30 s; mascot idle breath likewise.
- **Shutdown**: OnClosed must stop timers before Show on a closed window; Rust: destroy timers/hotkeys/listener before DestroyWindow.

### 5. CHECKS

#### checks/island.cs (port to clayo-core/tests/island_trigger.rs)
Setup: primary=PxRect(0,0,1920,1080); rest=Input(x960,y0,primary,scale1,all false,now0); Hold(t,i,from,to) = Update every 50 ms inclusive, returns last state; Peeking() = new trigger after Hold(rest,0,600); desk=rest{y500}; onPill=desk{y30,Over}; drag=rest{ButtonDown,Dragging}; paneA/paneB sources; needsA(A,"api-refactor",NeedsYou), doneA(A,Done), doneB(B,"docs-pass",Done), errorB(B,Error).
- Hold(rest,0,550) then Update(599) == Hidden; Update(600) == Peek
- Hold(rest,0,500); Update(y40,550); Hold(rest,600,1100)==Hidden; Update(rest,1200)==Peek
- Hold(rest{ButtonDown},0,2000)==Hidden
- Hold(rest{MovingOrSizing},0,2000)==Hidden
- Hold(rest{ButtonDown},0,1000); Update(rest,1050)==Hidden
- Hold(rest{Busy},0,2000)==Hidden
- Peeking(); Update(rest{Busy},700)==Hidden
- Hold(rest{x1080},0,600)==Peek
- Hold(rest{x1081},0,2000)==Hidden
- Hold(rest{x100},0,2000)==Hidden
- Hold(rest{y1},0,600)==Peek
- Hold(rest{y2},0,2000)==Hidden
- Peeking(); Update(y300,1000); Update(y300,1399)==Peek; Update(y300,1400)==Hidden
- Peeking(); Hold(onIsland{y30,Over},650,5000)==Peek; Update(y300,5050); Update(onIsland,5300); Hold(y300,5350,5700)==Peek
- Peeking(); Dismiss(); Hold(rest,700,3000)==Hidden; Update(y300,3050); Hold(rest,3100,3700)==Peek
- Second monitor PxRect(1920,-200,4480,1240) scale 1.5, s=(3200,-200): Hold(s,0,600)==Peek; x3380 -> Peek; x3381 (0..2000) -> Hidden; x3020 -> Peek; y0 (0..2000) -> Hidden; y-199 -> Peek
- IslandNote("5h","5h at 82%",Reserve).Urgent == false
- Hold(desk,0,1000); Notify(needsA); Update(1050)==Notify; Note==needsA; Hold(desk,1100,61100)==Notify; Resolve(A); Update(61150)==Hidden
- Notify(errorB); Hold(desk,0,30000); Dismiss(); Update(30050)==Hidden; Hold(30100,32000)==Hidden
- Peeking(); Notify(doneA); Update(rest,700)==Notify
- Notify(doneA); Update(desk,1000); Hold(1050,4950); Update(4999)==Notify; Update(5000)==Hidden
- Notify(doneA); Update(desk,0); Hold(over{y30,Over},3000,9000)==Notify; Update(desk,9050); Update(9449)==Notify; Update(9450)==Hidden
- Notify(needsA); Hold(desk{Busy},0,5000)==Hidden; Update(desk,5050)==Notify; Note==needsA
- Notify(doneA); Hold(Busy,0,10000); Update(desk,10050); Update(14000)==Notify; Update(14050)==Hidden
- Notify(doneA); Update(desk,0); Update(Busy,50)==Hidden
- Notify(errorB); Notify(doneA); Hold(Busy,0,1000); Update(1050); Note==errorB; Dismiss(); Update(1100); Note==doneA
- Notify(B NeedsYou); Notify(needsA); Update(0); Note==needsA
- Notify(doneA); Update(0); Notify(needsA); Update(50); Note==needsA; Notify(doneB); Update(100); Note==needsA
- Notify(needsA); Hold(Busy,0,1000); Resolve(A); Hold(desk,1050,2000)==Hidden
- Notify(doneB); Notify(needsA); Hold(ClayoActive,0,1000); Update(1050); Note==needsA; Dismiss(); Hold(desk,1100,2000)==Hidden
- Notify(needsA); Update(0); Update(ClayoActive,50)==Hidden
- Notify(needsA); Update(rest,0); Dismiss(); Hold(rest,50,3000)==Hidden
- Dwell: Update(rest,0) progress 0.0; Update(300) 0.5; Update(450) 0.75; Update(600) 0.0
- Hold(rest,0,300); Update(y40,350) progress 0; Update(ButtonDown,400) 0; Update(Busy,450) 0
- Hold(desk,0,1000) progress 0
- Hold(desk,0,1000); SetWorking(A,true); Update(1050)==Compact; Note==null; Hold(1100,11000)==Compact; Update(11050)==Hidden; Hold(11100,400000)==Hidden
- SetWorking(A); Hold(ClayoActive,0,5000)==Hidden; Update(desk,5050)==Compact
- SetWorking(A); Hold(Busy,0,5000)==Hidden
- SetWorking(A),(B); Update(0); SetWorking(A,false); Update(50)==Compact; SetWorking(B,false); Update(100)==Hidden
- SetWorking(A); Update(0); A false; Update(1000); B true; Hold(2000,60000)==Hidden; B false; A true; Update(179950)==Hidden; A false; Update(180000); A true; Update(180050)==Compact
- SetWorking(A); Update(0); SetWorking(B); Hold(50,400000)==Hidden
- SetWorking(A); Update(0); Update(ClayoActive,50); State==Hidden; Hold(desk,100,400000)==Hidden; A false; Update(400050); A true; Update(400100)==Compact
- SetWorking(A); Update(0); Hold(onPill,50,600)==Compact; Update(onPill,650)==Peek; Hold(onPill,700,20000)==Peek
- SetWorking(A); Update(0); Hold(rest,50,650)==Peek
- SetWorking(A); Update(0); Hold(onPill{ButtonDown},50,3000)==Compact; Hold(onPill,3050,3300)==Compact; Hold(desk,3350,3500); Hold(onPill,3550,4100)==Compact
- SetWorking(A); Update(0); Hold(onPill,9800,10000); State==Compact; Hold(onPill,10050,10400)==Peek
- SetWorking(A); Update(0); Notify(doneB); Update(50)==Notify; Note==doneB
- Notify(needsA); SetWorking(B); Update(0)==Notify
- SetWorking(A); Update(0); Dismiss(); Update(50)==Hidden; Hold(100,400000)==Hidden
- Greet(); Update(desk,0)==Compact; Greeting==true; Note==null; Hold(50,2450)==Compact; Update(2500)==Hidden; Greeting==false; Hold(2550,400000)==Hidden
- Greet(); Hold(Busy,0,30000)==Hidden; Update(desk,30050); Greeting==true
- Greet(); Hold(Busy,0,60000); Hold(desk,60050,400000)==Hidden
- Greet(); Update(ClayoActive,0); Hold(desk,50,400000)==Hidden
- Greet(); Update(desk,0); Hold(onPill,50,650)==Peek; Greeting==false
- Greet(); Hold(desk,0,2500); SetWorking(A); Update(2550); State==Compact; Greeting==false
- Hold(drag,0,200); State==Hidden; DwellProgress>0.5; Update(drag,250)==Drop
- Hold(drag{y39},0,300)==Drop
- Hold(drag{x1160},0,300)==Drop
- Hold(drag{y40},0,2000)==Hidden
- Hold(drag{x1201},0,2000)==Hidden
- Hold(rest{y20},0,2000)==Hidden
- Hold(rest{ButtonDown,MovingOrSizing},0,2000)==Hidden
- Hold(rest{ButtonDown},0,2000)==Hidden
- Hold(drag{y300},0,2000)==Hidden
- Hold(drag{Busy},0,2000)==Hidden
- Hold(drag,0,250); Hold(drag{y120,Over},300,3000)==Drop; Update(drag{y600},3050); Update(4549)==Drop; Update(4550)==Hidden
- Hold(drag,0,250); Hold(drag{y600},300,1000); Hold(drag{y120,Over},1050,3000)==Drop
- Hold(drag,0,250); Update(rest{y120,Over},300); Hold(rest{y120,Over},350,700)==Hidden
- Notify(doneB); Update(desk,0); Hold(drag,50,300)==Drop; Hold(desk,350,1200); State==Notify
- Hold(drag,0,250); Release(); Hold(rest,300,3000)==Hidden
- Hold(drag,0,250); Release(); Hold(rest,300,500); Hold(drag,550,900)==Drop
- Hold(drag,0,250); Dismiss(); Hold(drag,300,2000)==Hidden; Hold(rest,2050,2200); Hold(drag,2250,2600)==Drop
- Hold(drag,0,250); Hold(drag{y600},300,2000); Hold(drag,2050,2400)==Drop
- away=rest{y500}: Pick(0); Update(50)==Pick; Update(Busy,100)==Pick; Update(ClayoActive,150)==Pick; Notify(("pane","api",NeedsYou)); Update(200)==Pick; Update(14950)==Pick; Update(15000)==Hidden; Update(15050)==Notify
- Pick(0); Update(away{Over},14000); Update(away,28000)==Pick; EndPick(); State==Hidden; Hold(rest,28050,29000)==Hidden
Exit code = fail count.

#### checks/shots.cs (port to clayo-core/tests/screenshots.rs, temp dir)
- NewPath(dir, 2026-10-06 19:02:11) filename == "2026-10-06_19-02-11.png"
- after creating it, NewPath again == "2026-10-06_19-02-11-2.png"
- after creating that, NewPath == "2026-10-06_19-02-11-3.png"
- set first mtime = now − 7 d − 1 h, second = now − 1 d, notes.txt = now − 400 d; Prune(dir, now − 7 d): first deleted
- second kept
- notes.txt kept (only *.png pruned)
- Prune on non-existent dir does not error

#### checks/mascot.cs (visual, not pass/fail)
- Renders (static pose, never shown) Idle @256 -> wpf-idle.png, Peek @256 -> wpf-peek.png (glasses Y=9), Idle @44 -> wpf-idle-44.png, on background #0f1114, to out-dir (arg or %TEMP%\clayo-mascot)
- Reference: headless Edge screenshots of design/mascot/clayo-mascot.html with data-look="voxel" (idle and moves=['peek']), window 860x520, virtual-time-budget 600; FAIL only if a PNG was not written; SKIP if Edge/design file missing
- Rust port: example/ignored test rendering the tiny-skia mascot to PNGs (optionally golden-image compare).

## Phụ lục E — Terminal and pages


Paths are relative to `D:\Clayo\Clayo\src\Clayo` unless they start with `checks/`. Line numbers are from the files as read.
Abbreviations: TP = Terminal/TerminalPane.xaml.cs, TH = Assets/terminal.html, MW = MainWindow.xaml.cs, TT = Sessions/TranscriptTail.cs.

### 1. MAPPING

| C# / JS piece | Rust target | Purpose |
|---|---|---|
| Terminal/ConPty.cs `Native` (P/Invoke) | clayo-pty::sys | Bindings for CreatePseudoConsole, ResizePseudoConsole, ClosePseudoConsole, CreatePipe, ProcThreadAttribute*, CreateProcessW and TerminateProcess, via the `windows` crate. All `unsafe` stays here. |
| ConPty.cs `PtyProcess` | clayo-pty::PtyProcess | Spawns the child under ConPTY. Provides a read thread that sends raw bytes over a channel, `write(&[u8])`, `resize(cols, rows)`, an exit event, and Drop (kill, then tear down off-thread). |
| Terminal/PastedImages.cs | clayo-core::images (pasted_images) | Pure fs logic: `is_image`, `root()` (CLAUDE_CODE_TMPDIR or %TEMP% + `\claude`), `find_dir(root, session)`, `since(dir, t, pattern)`, `CODEX_PATTERN`. |
| TerminalPane.xaml (WebView2 host + LoadError TextBlock) | clayo-term::widget::TerminalView (egui) + clayo-app::pane | Pane UI. Background #14171C. Shows an error text when the pane fails to start. |
| TP `PaneStatus` enum | clayo-core::status::PaneStatus | Starting, Working, NeedsInput, Done, Error, Exited. |
| TP `Scan`, `Spaced`, the markers, `Answers`, `CountsAsWork`, `FromTimer`, `FromTurn` (pure static) | clayo-core::pane_status | Detects pane status from the output stream (byte scanning with a carry, timer rules). Unit-testable. |
| TP output batching (`_out`, `_flushTimer` 8 ms) | clayo-term::pump (or clayo-app::pane) | Coalesces pty reads into one parse-and-repaint per frame. |
| TP idle timer (400 ms) + `TypePrefill` + delayed AgentCommand typing (400 ms) | clayo-app::pane::lifecycle | Starts the shell, types the agent command, types the prefill, updates status on each tick. |
| TP `PasteFilesFromClipboard`, `InsertPaths`, `Pasted`, `Paste` | clayo-app::pane::paste (clipboard via `clipboard-win`/`arboard`) + clayo-core::paste (pure string building) | Ctrl+V with a file drop list, bracketed paste of image paths, quoted path insertion. |
| TP `ShowDoc`, `HideDoc`, DocLimit, `_docShown` | clayo-app::viewer::doc (pulldown-cmark + egui_commonmark) | Markdown viewer beside or over the terminal. Follows edits by mtime. |
| TP `ShowPage`, `PageNote`, PageLimit, `_pageShown` | clayo-app::viewer::page (open in the default browser in phase 1) | HTML page viewer. |
| TP `OnWebMessage` "link" case | clayo-app::links (ShellExecute via the `open` crate) | Opens http(s) URLs only, after a confirm prompt. |
| TP `Close` | clayo-app::pane Drop | Kills the pty and frees the view. |
| TH xterm `Terminal` options/theme | clayo-term::theme + clayo-term::config | Font, size, line height, cursor, scrollback 5000, palette. |
| TH WebGL addon `gpu(on)` | clayo-term::render (wgpu glyph atlas per pane; free it when hidden) | Drops GPU resources while the pane is hidden. |
| TH `sendTags` / `queueTags` | clayo-term::grid_scan (`image_tags(&Term) -> BTreeSet<u32>`) | Finds the `[Image #n]` tags on the bottom screen (not scrollback). Debounced 250 ms. |
| TH `attachCustomKeyEventHandler` | clayo-term::input::keymap | Ctrl+C copies when there is a selection (else ^C). Ctrl+V asks the host for a file paste, then pastes text. |
| TH `term.onData`/`onBinary` + xterm key encoding | clayo-term::input (alacritty-style key → bytes, mouse reports, focus reports, bracketed paste) | Turns input into bytes. |
| TH `doFit` + ResizeObserver 60 ms debounce | clayo-term::widget resize (debounced) → clayo-pty resize | Cell-grid fit and coalesced resize. |
| TH linkHandler (OSC 8) | clayo-term::hyperlink (alacritty_terminal keeps OSC 8 on cells) | Click → confirm → open in browser. |
| TH doc viewer (#md, split/full/close, grip 20–80%) | clayo-app::viewer::panel | Split (default 50%) or full overlay. Name bar. Drag grip. |
| TH DOC_HEAD CSS | clayo-app::viewer::doc style | Dark markdown styles (colors in §2). |
| TH pageframe postMessage protocol (link/ready/place) | clayo-app::viewer::page (deferred) | Keeps the scroll and slide place across reloads. |
| Assets/markdown/marked.min.js | pulldown-cmark (GFM: tables, strikethrough, tasklists) | Markdown → render. |
| Pages/PageHtml.cs `Seal`, `Csp`, `AssetsUrl` | clayo-core::pages::seal | Pure string transform: CSP meta after the doctype, asset links rewritten. Keep it for the browser fallback (write a sealed copy to a temp file with asset links pointing at a file:// or loopback copy). |
| Pages/PageFolder.cs `HideFromGit` | clayo-core::pages::hide_from_git | Appends `.clayo/` to `<repo>/.git/info/exclude`. |
| Assets/pages/clayo-page.js/.css, chart.umd, mermaid, templates/, page-guide.md | clayo-app assets (include_bytes!/ship beside exe) | Page runtime. Only usable in a browser. See §4. |
| Sessions/TranscriptTail.cs (not my file; it drives the doc/page chips) | clayo-core::transcript_tail | Doc, Page, Pages, PageWrites, PromptAt. Tested by checks/panefiles.cs. |
| MW chips (DocChip, PageChip + PageMenu, ImagesChip + ImagesPopup + ImageBig) | clayo-app::pane_header::chips | Header chips. |
| SessionLauncher `LaunchPlan`, `ResolveShell`, `ScrubInheritedSession` (not my file; feeds the pty) | clayo-core::launch | Shell command line and environment scrub. |

### 2. BEHAVIORS

#### Process lifecycle / environment
- [x] TRM-001 The pane spawns a shell, not the agent: `"<pwsh.exe full path>" -NoLogo` if pwsh.exe is on PATH, else `powershell.exe -NoLogo`. When claude exits you land at a prompt — Sessions/SessionLauncher.cs:30-35
- [x] TRM-002 ~400 ms after the pty starts, it types `AgentCommand + "\r"` once, so PowerShell does not eat the first characters — TP:446-459
- [x] TRM-003 The pty starts only when the view reports "ready" with its cols/rows (defaults 80x24; values <1 become 80/24) — TP:264-278, ConPty.cs:132-133
- [x] TRM-004 Working directory: `plan.WorkingDirectory` if it exists, else null (inherit Clayo's cwd) — ConPty.cs:188
- [x] TRM-005 Environment: the child inherits Clayo's own process environment (lpEnvironment = NULL, CREATE_UNICODE_ENVIRONMENT). Nothing is added. TERM and COLORTERM are not set by Clayo — ConPty.cs:186-187
- [x] TRM-006 Before any spawn, Clayo deletes these from its own environment: CLAUDECODE, CLAUDE_CODE_CHILD_SESSION, CLAUDE_CODE_ENTRYPOINT, CLAUDE_CODE_SESSION_ID, CLAUDE_CODE_MESSAGING_SOCKET, CLAUDE_CODE_MESSAGING_TOKEN, CLAUDE_PID. ANTHROPIC_* and CLAUDE_EFFORT are kept — SessionLauncher.cs:48-63
- [x] TRM-007 The session id is allocated up front (new GUID) for New and Fork when the agent supports it. Resume uses the existing id — SessionLauncher.cs:75-88; TP:113
- [x] TRM-008 Prefill text (e.g. from "open with prompt") is typed once, without Enter, at the first quiet moment (Done, or 1.5 s of silence for a Codex pane with a known turn), only if output arrived after the command was typed — TP:139,150,469-474
- [x] TRM-009 When the shell exits: pending output is flushed, status becomes Exited, the idle timer stops, and a dim gray line `[shell exited]` (SGR 38;5;244) is printed after "\r\n". No auto-restart — TP:424-430, TH:182-184
- [x] TRM-010 If the spawn fails: status Exited, and the notice "Couldn't start the shell: <msg>" is printed in the terminal — TP:437-442
- [~] TRM-011 If the view fails to load: the text "Couldn't load the terminal: <msg>" replaces the pane, and other panes keep running — TP:177-183 (D10)
- [~] TRM-012 If the renderer crashes (RenderProcessExited): the page reloads automatically. On the new "ready", the pty is not restarted: the viewer state is reset and the pty is resized to the same size, which forces a ConPTY repaint — TP:232-236, 268-274 (D10)
- [x] TRM-013 Closing a pane: TerminateProcess(exit 0) first, then ClosePseudoConsole, stream disposal and handle closing on a background task (ClosePseudoConsole can block before Win11 24H2). The WebView is disposed (it otherwise leaked ~140 MB/pane) — ConPty.cs:249-290, TP:685-698
- [x] TRM-014 The read loop keeps reading to EOF even after dispose (ClosePseudoConsole waits for its output to be drained) but drops the data. Exited is not raised after dispose — ConPty.cs:207-225

#### Rendering / font / theme
- [x] TRM-015 Font: `"Cascadia Code", "Cascadia Mono", Consolas, monospace`, 13 px, lineHeight 1.25, letterSpacing 0 — TH:82-85
- [x] TRM-016 Cursor: bar, blinking. Cursor color #d4a24c, cursorAccent #14171c — TH:86-87,102-103 (the bar rests solid after 5 s idle: D14)
- [x] TRM-017 Scrollback: 5000 lines (20000 cost ~29 MB/pane) — TH:88-90
- [x] TRM-018 Theme: background #14171c, foreground #c8cdd6, selectionBackground #2f3947, black (ANSI 0) #14171c, brightBlack (ANSI 8) #5a6472 — TH:99-106
- [x] TRM-019 The other ANSI colors are xterm.js 6 defaults (Tango), from the vendored xterm.js DEFAULT_ANSI_COLORS: red #cc0000, green #4e9a06, yellow #c4a000, blue #3465a4, magenta #75507b, cyan #06989a, white #d3d7cf; brightRed #ef2929, brightGreen #8ae234, brightYellow #fce94f, brightBlue #729fcf, brightMagenta #ad7fa8, brightCyan #34e2e2, brightWhite #eeeeec. Indexes 16–255 are the standard xterm 6x6x6 cube plus a 24-step gray ramp. Truecolor is supported. "Claude Code paints its own colours" — Assets/xterm/xterm.js (DEFAULT_ANSI_COLORS), TH:98
- [x] TRM-020 Scrollbar (xterm-drawn): slider #333a45, hover/active #464f5d — TH:107-110
- [x] TRM-021 Terminal padding: 10px top, 4px right, 6px bottom, 12px left. Page background #14171c — TH:12-26
- [x] TRM-022 The pane/host background is #14171C before load (no white flash) — TerminalPane.xaml:5,7
- [x] TRM-023 The WebView is forced to a dark color scheme. Zoom, the status bar and the default context menu are off. Browser accelerators (F5, Ctrl+P…) are off in release; F12 devtools work in debug only — TP:203-211, 228 (egui zoom off; devtools: D10)

#### Input / keys
- [x] TRM-024 Every keystroke is sent as UTF-8 bytes (xterm onData → TextEncoder). Mouse reports, focus reports and bracketed-paste payloads go as raw bytes (onBinary, char & 0xFF) — TH:349-358
- [x] TRM-025 Key → VT encoding is xterm.js's default (arrows, F-keys, Alt as an ESC prefix, Ctrl+letter → C0, application cursor mode, etc.). Clayo customizes only the keys below — TH:401-419
- [x] TRM-026 Ctrl+C (also Ctrl+Shift+C; any Ctrl without Alt/Meta and code KeyC) with a terminal selection: copies the selection to the clipboard as text, clears the selection, and sends nothing to the child. The next Ctrl+C interrupts — TH:402-408
- [x] TRM-027 Ctrl+C with no selection: falls through to xterm and sends ^C (0x03) to the child — TH:403,418
- [x] TRM-028 Ctrl+V (also Ctrl+Shift+V): xterm does not send ^V. The host is asked (`paste`) to handle clipboard files, and the browser's native paste runs, which xterm turns into a text paste (bracketed if the app enabled mode 2004, CRLF/LF → CR) — TH:409-417
- [x] TRM-029 Host side of Ctrl+V: does nothing if the clipboard has text or has no file drop list. Otherwise, each image file (png/jpg/jpeg/gif/webp/bmp) is written as a bracketed paste `ESC[200~<path>ESC[201~` plus a space, so Claude and Codex turn it into an `[Image #n]` attachment. Other files are typed as plain text: space-separated, double-quoted if the path contains a space, with one trailing space and no Enter — TP:342-368, 374
- [x] TRM-030 In a Codex pane, each image pasted that way is remembered in `CodexImages` (path, now) for the image chip — TP:353
- [x] TRM-031 A raw bitmap on the clipboard (not a file) is not handled by Clayo. Ctrl+V does not send ^V, so the agent's own image-paste key must be used (e.g. Claude Code's Alt+V passes through as ESC v). *Inference: there is no code path for bitmap clipboard data* — TH:409-417, TP:344
- [x] TRM-032 Shift+Enter, Ctrl+Enter and Alt+Enter are not special-cased. xterm defaults apply: Shift+Enter → "\r" (same as Enter), Alt+Enter → ESC "\r". Ctrl+Shift+other is not special-cased — TH:401-419
- [x] TRM-033 Right-click: no context menu (disabled), no paste on right-click. xterm's rightClickSelectsWord is false on Windows — TP:203
- [x] TRM-034 Dropping a file or link on the pane does not navigate (AllowExternalDrop=false). Drop-to-insert is handled elsewhere (island/MainWindow) — TP:195
- [x] TRM-035 Keys typed while a question or error is showing "answer" it (status can leave NeedsInput/Error), except sequences longer than 2 bytes that start with `ESC [` (arrows, focus reports, mouse reports). A bare ESC, Enter or a digit does answer — TP:284-292, 517
- [x] TRM-036 In a Codex pane with a known turn, answering a NeedsInput sets status to Working right away — TP:291

#### Selection / copy
- [x] TRM-037 Selection uses xterm defaults: drag selects, double-click selects a word, triple-click selects a line, Shift+click extends, Alt+drag makes a column (block) selection, and Shift+drag forces a selection while the app has mouse reporting on. Selection color #2f3947 — TH:104 (rest xterm defaults)
- [x] TRM-038 Copy is done only by Ctrl+C (or Ctrl+Shift+C) with a selection. There is no copy-on-select and no right-click copy. The selection is cleared after copying — TH:403-407

#### Resize
- [x] TRM-039 The view fits cols/rows to the container (FitAddon). The resize is debounced 60 ms after the last ResizeObserver callback, and "r" is sent only when cols/rows changed — TH:360-378
- [x] TRM-040 The host ignores a resize while the window is minimized (a false 124→105 col reflow was seen) and ignores a same-size resize. Otherwise it calls ResizePseudoConsole and records `_resizedAt` — TP:297-309
- [x] TRM-041 Output within 500 ms after a resize, or within 250 ms after a key, does not count as "work" (it is a repaint or echo) but does reset the quiet timer — TP:387-388, 396-402
- [x] TRM-042 Dragging the doc-split grip resizes the terminal through the same debounce — TH:325-341

#### Links
- [x] TRM-043 OSC 8 hyperlinks in the output: on click, the confirm dialog "Open this link in your browser?\n\n<uri>" appears. If confirmed, the link opens in the default browser — TH:93-97
- [x] TRM-044 The host opens only absolute http/https URIs (file: and other schemes are dropped) with ShellExecute — TP:326-331
- [x] TRM-045 Plain-text URLs (no OSC 8) are not linkified (the web-links addon is not loaded) — TH:69-71
- [x] TRM-046 Links in a doc: http(s) only, same confirm, open in the browser. Other links do nothing (default is prevented) — TH:274-280
- [~] TRM-047 Links in a page: same confirm, http(s) only. `#anchor` links scroll inside the page — TH:314-316, Assets/pages/clayo-page.js:457-471 (D3)
- [x] TRM-048 New windows are never opened inside Clayo. Top navigation outside `https://ccx.assets/` is cancelled. Frame navigation other than `about:` (srcdoc) is cancelled — TP:217-230

#### Focus
- [x] TRM-049 On load the terminal takes focus. After a chip click, a viewer close, a pane switch, closing the image viewer or attaching files, focus returns to the terminal (`FocusTerminal` = view focus plus a focus message) — TH:380-384, 176-178, 291; MW:1102,1330,1467,1485,1502,1661
- [x] TRM-050 Focus in/out reports (CSI I/O) are sent by xterm when the app enables mode 1004. They do not count as an answer — TH:354, TP:517

#### Visibility / perf
- [x] TRM-051 A hidden pane (another session picked, Settings open, window in the tray) drops its WebGL renderer and recreates it when shown again. A pane hidden before its page was ready is told after "ready". On WebGL context loss it falls back to the DOM renderer — TP:163,266; TH:118-137 (DOM fallback: D10)
- [x] TRM-052 Output is batched: pty reads are added to a buffer under a lock, and the first read schedules an 8 ms dispatcher timer at Normal priority (not Background, so key echo is not delayed, issue #21). The whole buffer is posted as one base64 message, so a redraw split over several reads paints in one go (no flicker) — TP:411-421, 155-159, 581-592 (parsed on the read thread, one repaint per frame: D13)

#### Status detection (drives the sidebar dot, island, notifications)
- [x] TRM-053 The output stream is scanned per chunk. `ESC[1C` is replaced with a space (ConPTY sends spaces as cursor moves). A carry of (max marker len − 1) = 16 bytes joins markers split across reads — TP:497-508, 538-550
- [x] TRM-054 "API Error:" → Error. "Do you want to" → NeedsInput. "Would you like to" → NeedsInput in Codex panes only. Error has priority over the ask markers. A scan hit resets `answered` and records askedAt (local time) — TP:481-488, 403-409
- [x] TRM-055 Idle timer, every 400 ms: if Exited, skip. If a Codex turn is known: only type the prefill after 1.5 s of quiet. Else: burst = chunks counted as work in the last 900 ms (the queue keeps the last 12). NeedsInput/Error stay until answered. burst ≥ 3 → Working. Quiet > 1500 ms → Done (and type the prefill). Otherwise unchanged — TP:128-151, 523-531
- [x] TRM-056 Codex panes, once the transcript reports a turn: Working/Done comes from the transcript (`FromTurn`). Exited never changes. NeedsInput stays unless answered and the transcript line is newer than the question — TP:563-579

#### Image chip (pane header; MW logic using TP state)
- [x] TRM-057 The page reports the set of `[Image #n]` tags visible on the bottom screen (rows baseY..baseY+rows, not scrollback), 250 ms after output is parsed, and only when the set changes (comma-joined, sorted) — TH:198-228, TP:320-324
- [x] TRM-058 Polled every 1 s while the window is visible: images "since" = the last prompt sent after the pane opened (TT.PromptAt), else the time the pane opened — MW:266,1342-1345
- [x] TRM-059 Claude: images dir = `<CLAUDE_CODE_TMPDIR or %TEMP%>\claude\<any project>\<sessionId>\images`. Files are images with mtime > since, sorted by numeric file name (the tag), then by mtime — PastedImages.cs:21-53, MW:1357-1362
- [x] TRM-060 Codex: loose `%TEMP%\codex-clipboard-*` images newer than since, claimed by the active pane only (a global claimed set). Images Clayo pasted are added too. Entries older than since, or deleted, drop out. Tag = position + 1 — PastedImages.cs:17, MW:1348-1355, 1364-1368
- [x] TRM-061 Only images whose tag is still on screen are shown. Deleting `[Image #2]` from the prompt removes it from the chip — MW:1366-1369
- [x] TRM-062 The chip (icon + count) is hidden when there are 0 images. Hovering opens a popup of 90 px-high thumbnails labelled `#n`, which auto-hides 300 ms after the mouse leaves. A thumbnail click opens the full-size image over the pane (Esc, a click or the close button closes it, then focus returns to the terminal). A chip click opens the newest image full size. A newly added image flashes the popup for 1.5 s, only while Clayo is the active window — MW:1370-1470
- [x] TRM-063 Thumbnails are decoded with OnLoad and the file is released at once, so the agent's file is never locked. Undecodable formats show the tag only — MW:1418-1437

#### Doc chip / doc viewer (.md written by the session)
- [x] TRM-064 Detection (TT): the newest `.md` that the session Write/Edit/MultiEdit-ed (Claude) or apply_patch Add/Update-d or FileChange-d (Codex), resolved against the session folder. It must be inside that folder and still exist. A doc from before the pane opened (a resumed session) also counts — TT:87-127,169-178
- [x] TRM-065 The chip is visible when a doc is known. Tooltip: "<name>\n<full path>\nClick to read it here, again to close it." Click toggles show/hide, then refocuses the terminal — MW:1471-1486
- [x] TRM-066 The viewer opens in split mode (50% of the pane width, on the right, terminal shrinks and reflows) unless it is already open. "◧ Split" / "⛶ Full" (full = overlay over the terminal; the terminal keeps its size) / "✕" close. The active mode button is amber #d4a24c. The name bar shows the file name — TH:28-58, 255-269
- [x] TRM-067 The split grip (8 px over the left border, col-resize, amber 2 px line on hover/drag) sets the doc width to 20–80%. The width is kept for the next open within this page load — TH:31-36, 325-341
- [x] TRM-068 Follow edits: on every transcript change, if the doc is open, the current Doc is shown again if its path or mtime changed (an Edit reaches the transcript before the file). The same doc keeps its scroll position; a different doc starts at the top — MW:658-669, TP:613-617, TH:261-264
- [x] TRM-069 Files over 2 MB show "*<name> is too big to show here.*". A read error shows "*Couldn't read <name>: <msg>*" — TP:603-629
- [~] TRM-070 Rendered with marked (GFM) in a sandboxed frame with no scripts, CSP `default-src 'none'; style-src 'unsafe-inline'; img-src https: data:` (remote https images load). Raw `<pre>` with escaped text is the fallback if marked is missing — TH:60-62, 238-249, 269 (D4)
- [x] TRM-071 Doc CSS: body 14px/1.6 Segoe UI, padding 18 24 40, bg #14171c, fg #c8cdd6. h1–h4 #eceef1 (lh 1.3, margin 1.4em 0 .5em). h1/h2 have a bottom border #262c35. Links #d4a24c. Code 12.5px Cascadia Code on #1b1f26 with radius 3. pre #1b1f26, padding 10 12. blockquote: left border 3px #333a45, color #7a828f. Table cells: border #262c35, padding 4 10. Images max-width 100%. hr #262c35. Bar: 12px Segoe UI #7a828f, border #262c35, button hover bg #222731 — TH:38-45, 243-248
- [x] TRM-072 Closing the viewer (✕) tells the host (`mdclosed`) and refocuses the terminal. A host-side close (`mdclose`) does the same without telling the host back — TH:285-292, TP:315-318, 636-641

#### HTML pages
- [x] TRM-073 What: a single HTML file an agent writes at `<project>/.clayo/pages/<name>.html` (report/dashboard/slides), using clayo-page.css/js. Charts are JSON in `<script type="application/json" class="chart">` (Chart.js: bar/line/area/pie/doughnut/scatter, stacked/horizontal, xLabel/yLabel). Diagrams are `<pre class="mermaid">`. Tables are sortable with `data-sortable`. Decks use `.deck > .slide` (16:9) — Assets/pages/page-guide.md:1-153 (in the default browser with the page assets, D3)
- [x] TRM-074 Detection (TT): an .html Write/Edit (or Codex patch/FileChange) inside `<folder>\.clayo\pages\`. A page written again moves to the end. `Pages` lists each one once, newest last. PageWrites counts every write — TT:118-125
- [x] TRM-075 A new page write after the first transcript read opens the page in the viewer automatically. A page from history (the first read) only appears in the chip. On a page write, `.clayo/` is appended to `.git/info/exclude` of the enclosing repo (searched upward). If the page is open and the file still exists, it is shown again on change (mtime) — MW:663-677, Pages/PageFolder.cs:15-32
- [x] TRM-076 Page chip: visible when any page is known. Tooltip: "<name>\n<path>\nClick to see it here, again to close it. Right-click for the session's other pages." Click toggles. The right-click menu lists the session's existing pages, newest first, with the open one checked — MW:1489-1515
- [~] TRM-077 The page shows in the same panel as the doc (one at a time). The same page again keeps its scroll y and slide (reported by the page via postMessage `place`, debounced 150 ms). A different page starts at the top — TH:294-323, clayo-page.js:433-453 (D3)
- [~] TRM-078 Sealing: a CSP meta is inserted straight after the doctype (or at offset 0 if there is none), so the page cannot comment it out. CSP = `default-src 'none'; script-src 'unsafe-inline' https://ccx.assets https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; style-src` (same list)`; img-src data: blob: https://ccx.assets; font-src data: https://ccx.assets; connect-src 'none'; form-action 'none'`. `href|src = "./clayo-page.css|js"` (either quote style, optional ./) is rewritten to `https://ccx.assets/pages/clayo-page.(css|js)`. Nothing else is touched — Pages/PageHtml.cs:43-81 (D3: sealed for the browser copy)
- [~] TRM-079 Frame sandbox: `allow-scripts` only (opaque origin; it cannot reach the host, files or the network). The doc frame is `allow-same-origin` with no scripts — TH:60-66 (D3)
- [~] TRM-080 Over 5 MB: a dark note "<name> is too big to show here." A read error: "Couldn't read <name>: <msg>" (HTML-encoded, on #14171c, color #7a828f) — TP:643-677 (D3)
- [~] TRM-081 Page runtime: dark/light via `light-dark()` tokens (Clayo forces dark). Charts and Mermaid redraw on theme change. Decks: ←/→/↑/↓/PgUp/PgDn/Space/Home/End and nav buttons, with a "n / N" counter. Tables: numeric columns right-aligned, header click or Enter/Space sorts ascending then descending — clayo-page.js:337-431, 480-482; clayo-page.css:1-184 (D3)
- [~] TRM-082 Not wired in the C# code: page-guide.md and templates/ are shipped in Assets, but no code copies them into `.clayo/templates` or tells the agent about them. The PageHtml doc mentions `clayo --page-check`, but no such CLI handler exists in src — grep of src (no hits) (D9)
- [x] TRM-083 IME composition (Vietnamese Telex, CJK IMEs): preedit shown at the cursor, candidate window placed at the cursor cell, committed text sent as UTF-8 (xterm.js behavior) — Assets/xterm/xterm.js (composition helper)
- [x] TRM-084 Viewer tooltips: grip "Drag to resize", Split "Half and half", Full "Fill the pane", ✕ "Back to the terminal" — Assets/terminal.html:52-57

### 3. HARD POINTS

- **ConPTY creation**: `CreatePseudoConsole(size, inputRead, outputWrite, dwFlags = 0, &hPC)`. No PSEUDOCONSOLE_INHERIT_CURSOR or other flags. Pipes come from `CreatePipe(..., nSize = 0)` (default buffer size), are synchronous (not overlapped) and not inheritable. The ConPTY-side handles (inputRead, outputWrite) are closed right after CreatePseudoConsole, or EOF never arrives — ConPty.cs:139-156
- **Attribute list**: InitializeProcThreadAttributeList size probe (first call fails by design). `UpdateProcThreadAttribute(PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE = 0x20016, lpValue = hPC itself (not a pointer to it), cb = sizeof(HANDLE))` — ConPty.cs:161-178
- **CreateProcessW**: appName NULL, a mutable copy of the command line, bInheritHandles FALSE, flags EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT, env NULL, STARTUPINFOEX.cb = sizeof(STARTUPINFOEX). The STARTUPINFO std handles are left zero — ConPty.cs:180-190
- **Read**: a dedicated thread "ConPTY read" with a 16 KB buffer. Chunks are copied out and sent raw (never decoded, because UTF-8 and escape sequences can split across reads). Pipe IO and ObjectDisposed errors count as EOF — ConPty.cs:194-226. In Rust: a std::thread blocking ReadFile into a channel, then wake egui (`ctx.request_repaint()`) and feed `alacritty_terminal::vte::ansi::Processor` on the UI thread or a parser thread holding `Arc<FairMutex<Term>>` (as alacritty does).
- **Write**: unbuffered, one write and flush per message. Errors are swallowed after dispose. Text input is UTF-8 — ConPty.cs:228-240
- **Resize**: `ResizePseudoConsole(hPC, COORD{cols, rows})`, skipped if <1 or disposed. The caller debounces (60 ms in the view), skips while minimized and skips same-size. A same-size resize is used on purpose to force a repaint after a renderer reload — ConPty.cs:242-247, TP:272, 297-309. In Rust: debounce resizes from the widget's layout (≥60 ms) and resize alacritty Term and ConPTY together.
- **Teardown order**: TerminateProcess → (off-thread) ClosePseudoConsole → close streams → DeleteProcThreadAttributeList/free → close hThread/hProcess. The read thread must keep draining until EOF or ClosePseudoConsole hangs (before Win11 24H2) — ConPty.cs:207-290
- **Output batching to the UI**: accumulate under a lock, then flush once per ~8 ms tick on the UI thread at Normal priority. One message per flush prevents half-painted frames — TP:411-421,581-592. Native equivalent: the parser thread processes bytes as they come, the renderer repaints at most once per frame (request_repaint_after(~8 ms) on first dirty), and it never repaints mid-redraw. Optionally honor synchronized output (DEC mode 2026, which alacritty_terminal supports) for Claude Code's full-screen redraws.
- **Status scan on raw bytes**: it runs on the read thread, before batching, with a 16-byte carry and the ESC[1C→space substitution. Keep it as pure functions in clayo-core (tests in §5).
- **Encoding**: bytes in both directions. Input keys are UTF-8. A paste goes through xterm (CRLF/LF → CR, wrapped in ESC[200~ … ESC[201~ only if the app enabled DECSET 2004). Image paths are always wrapped in bracketed-paste markers by the host, whatever the mode — TP:374. Vietnamese and emoji width depend on the terminal's wcwidth. ConPTY itself re-encodes, so use alacritty's unicode width tables.
- **Perf tricks**: WebGL only on visible panes (GPU glyph textures per pane freed when hidden), scrollback cut to 5000, a 250 ms debounce on the tag scan, a 60 ms resize debounce, minimize-resize suppression, and Web.Dispose on close (~560 MB for 4 leaked panes). In Rust: do not render, or keep atlases for, hidden panes. Share one glyph atlas across panes (one wgpu device) instead of per-pane.
- **Image paste to claude/codex**: Clayo never sends image bytes. It sends the *path* as a bracketed paste (`ESC[200~C:\path\x.png ESC[201~ `). Both agents turn a pasted image path into `[Image #n]`, while a typed path stays text. Claude copies the image into `%TEMP%\claude\<proj>\<session>\images\<n>.png` (or CLAUDE_CODE_TMPDIR). Codex uses the file in place, and its own clipboard pastes land in `%TEMP%\codex-clipboard-*.png`. The screenshot hotkey (Ctrl+Alt+S, island) and Explorer-copied files both use `Paste(path)`. A bitmap clipboard paste is left to the agent's own key — TP:342-377, PastedImages.cs:5-12, MW:1531-1554
- **Ctrl+V double path**: in the web version, Ctrl+V triggers both a host file-paste check and the browser text paste. In native code, check the clipboard on Ctrl+V: if it has CF_UNICODETEXT, do a text paste (bracketed if mode 2004, with newline normalization). Else if it has CF_HDROP, do the file logic. Else do nothing (or optionally forward to the agent; ask the user).
- **Image tag scan**: needs read access to the visible grid rows (`Term::grid()` bottom screen, not scrollback). Regex `\[Image #(\d+)\]` per row, with wrapped rows translated separately (translateToString(true) per line, trimmed) — TH:205-222
- **Status vs. key echo**: TP keeps `_keyAt` from input messages. Native input must update the same timestamp before writing to the pty — TP:284

### 4. DEFER CANDIDATES

- **HTML pages (Chart.js, Mermaid, clayo-page.js runtime, decks, sortable tables)**: these need a real browser engine. Fallback (as the plan says): on a page write or chip click, write `Seal(html)` to `%LOCALAPPDATA%\Clayo\pages\<hash>\<name>.html` with clayo-page.css/js, chart.umd.min.js and mermaid.min.js copied beside it, the asset links rewritten to relative or file:// URLs (not https://ccx.assets), and the CSP adjusted to allow `'self'`/file. Then open it in the default browser (ShellExecute). Without the sandbox iframe, the CSP is the only seal (connect-src 'none', form-action 'none'). Note that browsers treat file:// origins differently. The alternative is a 127.0.0.1 server, but the plan says the only network use is GitHub Releases, so prefer file://. Lost: the inline split view, scroll/slide place keeping, and in-app link confirm. Keep the chip, the right-click page list, auto-open on a new write (opens the browser — maybe ask the user whether auto-open should launch a browser), and HideFromGit.
- **Markdown doc viewer**: portable natively with pulldown-cmark + egui_commonmark (tables, code blocks, links via confirm dialog). Remote https images (allowed today) need an HTTP fetch, which the network invariant forbids, so show a placeholder or alt text and allow data: images only. Raw HTML inside the markdown is not rendered (it was not executed before either; marked passes HTML through, but the sandbox blocked scripts). Mermaid/code fences render as code.
- **Split/full viewer with drag grip**: easy in egui (SidePanel resizable 20–80%, or a central overlay).
- **OSC 8 links**: alacritty_terminal stores hyperlinks per cell. Needs hover underline, click, and a native confirm (MessageBoxW or egui modal).
- **Plain-URL detection**: was not present, so do not add it (behavior parity).
- **WebView reload-on-crash**: not applicable. Replace with "renderer error → recreate the widget, pty untouched".
- **DOM fallback renderer / WebGL context loss**: if wgpu fails, egui's glow backend is a fallback option.
- **xterm-specific selection niceties** (Alt column select, triple-click line, Shift-drag override over mouse mode, word separators): alacritty_terminal has Selection types (Simple, Block, Semantic, Lines). Map them all.
- **Pages templates / page-guide.md**: not wired in C# today. Ship the assets, no behavior to port.

### 5. CHECKS (→ cargo test)

#### checks/pages.cs (PageHtml::seal, CSP, PageFolder::hide_from_git)
Input `page` = `"<!doctype html>\n<html lang=\"en\"><head>\n<meta charset=\"utf-8\">\n<link rel=\"stylesheet\" href=\"clayo-page.css\">\n</head><body>\n<script>var a = 1;</script>\n<script src=\"clayo-page.js\"></script>\n</body></html>"`, `Meta` = `<meta http-equiv="Content-Security-Policy"`, s = seal(page).
- s starts with `"<!doctype html>" + Meta` — pages.cs:20
- index(Meta) < index("<script") in s — :21
- Meta occurs exactly once in s — :22
- s ends with `<script>var a = 1;</script>\n<script src="https://ccx.assets/pages/clayo-page.js"></script>\n</body></html>` — :23
- seal("<h1>hi</h1><script>x()</script>") starts with Meta (no doctype → offset 0) — :25
- seal("  <!DOCTYPE HTML><p>") starts with "  <!DOCTYPE HTML>" + Meta (case-insensitive, leading whitespace kept) — :26
- seal("<!doctype html><!-- <head> --><script>fetch('https://example.com')</script>"): index(Meta) < index("<!--") — :28-29
- CSP contains "connect-src 'none'" — :32
- CSP contains "form-action 'none'" — :33
- CSP starts with "default-src 'none'" — :34
- CSP contains "img-src data: blob: https://ccx.assets;" — :35
- s contains `href="https://ccx.assets/pages/clayo-page.css"` — :38
- seal("<link href='./clayo-page.css'><script src = './clayo-page.js'></script>")[Meta.len..] contains `href='https://ccx.assets/pages/clayo-page.css'><script src = 'https://ccx.assets/pages/clayo-page.js'>` (single quotes, ./ stripped, spaces around = kept) — :39
- Left alone (seal(x) ends with ">" + x) for each: `<link rel="stylesheet" href="other.css">`, `<link rel="stylesheet" href="my-clayo-page.css">`, `<link rel="stylesheet" href="sub/clayo-page.css">`, `<script src="https://cdn.jsdelivr.net/npm/d3@7.9.0/dist/d3.min.js"></script>`, `<a href="https://example.com/clayo-page.js">x</a>`, `<p>clayo-page.css</p>` — :40-49
- hide_from_git: tmp/repo with .git dir and repo/src/app. hide_from_git(repo) → `.git/info/exclude` == ".clayo/\n" (creates the info dir) — :61-62
- Called again → still ".clayo/\n" (idempotent; line match is trimmed) — :63-64
- Exclude preset to "# local\n*.tmp" (no trailing newline), hide_from_git(repo/src/app) → "# local\n*.tmp\n.clayo/\n" (walks up to the repo root, adds a newline first) — :66-68
- A plain non-repo folder → nothing written in it (the folder stays empty) — :70-73

#### checks/panefiles.cs (TranscriptTail docs/pages/prompt; PastedImages)
Helpers: Claude Write line `{"type":"assistant","timestamp":T,"message":{"role":"assistant","content":[{"type":"tool_use","name":"Write","input":{"file_path":P,"content":"x"}}]}}`. Prompt `{"type":"user","timestamp":T,"message":{"role":"user","content":"see docs/a.md"}}`. ToolResult = user line with content [{"type":"tool_result",...}]. Codex Patch `{"timestamp":T,"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Add File: P\n+# hi\n*** End Patch\n"}}`. folder = tmp/proj, opened = now, timestamps in `yyyy-MM-ddTHH:mm:ss.fffZ`.
- Write of folder/GONE.md (never created) at opened−6 min → read() == false — panefiles.cs:44-46
- Append Write OLD.md (exists) at −5 min + Prompt at −4 min → read() true and doc == folder/OLD.md (a pre-open doc counts) — :48-49
- prompt_at == MIN (a pre-open prompt is ignored) — :50
- Write docs/PLAN.md at +1 s → read() true — :55-56
- doc == full path folder\docs\PLAN.md — :57
- read() again with nothing new → false — :58
- Write of dir/elsewhere.md (outside the folder) → false — :60-61
- Write of folder/code.cs (not markdown) → false — :62-63
- Write HALF.md without a trailing newline → false (partial line waits) — :65-66
- Append "\n" → read() true, doc == folder/HALF.md — :67-68
- prompt_at still MIN — :70
- Prompt at opened+4 s, then ToolResult at +9 s → prompt_at == +4 s (UTC) — :71-74
- Prompt at +10 s with `"isMeta":true` → prompt_at unchanged (+4 s) — :75-77
- Pages: create .clayo/pages/a.html, b.html and site/index.html. page == None initially — :80-83
- Write pageA at +10 s → read() true, page == pageA — :84-85
- doc still == HALF.md — :86
- Write site/index.html → read() false — :87-88
- Write pageB (+12 s), then pageA (+13 s) → pages file names == "b.html,a.html" (rewrite moves to end, deduped) — :89-91
- page_writes == 3 — :92
- Codex: Patch "notes/README.md" (relative) at +1 s → read() true, doc == folder/notes/README.md — :95-98
- Patch "../escape.md" → false — :99-100
- Patch with the full Windows path folder\notes\README.md (JSON-escaped backslashes; "\notes" must not split at \n) → doc == that path — :102-103
- Code mode: exec custom_tool_call whose input is JS containing a doubly-escaped apply_patch for docs/scratch/codex-test.md, followed by event_msg item_completed FileChange {changes:{<full path>:{type:add}}} → doc == folder/docs/scratch/codex-test.md — :106-109
- Patch ".clayo/pages/a.html" → page == pageA — :111-112
- Patch "site/index.html" → false — :113-114
- Codex user message `<environment_context>cwd</environment_context>` (response_item message role user input_text) → prompt_at == MIN — :116-120
- Codex user message "what is in this screenshot?" at +4 s → prompt_at == +4 s — :121-124
- PastedImages: root = tmp/claude. find_dir(root,"sess-1") == None before the dir exists — :127-129
- After creating root/C--proj/sess-1/images → find_dir == that path — :130-131
- Files 1.png (mtime since−60 s), 10.png (+5), 2.webp (+5), notes.txt (+5) → since(images, since) names == "2.webp,10.png" (images only, newer only, numeric tag order) — :133-140
- since(nonexistent dir) → empty — :141
- Codex temp: codex-clipboard-b.png (+8), codex-clipboard-a.png (+4), codex-clipboard-old.png (−60), other.png (+5) → since(temp, since, "codex-clipboard-*") == "codex-clipboard-a.png,codex-clipboard-b.png" (non-numeric names → order by mtime) — :144-154

(Line refs in checks/panefiles.cs and checks/prompt.cs below may be off by up to 3 lines. Exact: prompt.cs Checks are at 71, 73, 76, 79, 81, 85, 87, 89, 91, 98, 100, 106, 108, 110.)

#### checks/prompt.cs (pane status state machine: scan / counts_as_work / from_timer / answers / from_turn)
Simulator `Run(output[(ms,text)], until, keys[(ms,text)])`: status starts Working, answered=false, t0=2026-10-16 12:00 UTC, resizedAt=MIN, carry empty, recent queue (max 12), last=−∞. Every 400 ms tick up to `until`: process events with at ≤ tick in time order. For a key: keyAt=t0+at, and if answers(bytes) then answered=true. For output: last=at; if counts_as_work(t0+at, MIN, keyAt) push at; if scan(bytes,&carry) is Some(s) then status=s, answered=false. Then burst = count(tick−t < 900) and apply from_timer(status, burst, tick−last, answered). Dialog(from) = 5 chunks at +0/40/80/120/160, with "Do\e[1Cyou\e[1Cwant\e[1Cto\e[1Cproceed?" in the 3rd. Burst(from,n) = n chunks every 60 ms of "\e[2K✳ Running…\r".
- Burst(0,10)+Dialog(1000), until 5000 → NeedsInput — prompt.cs:73-74
- API error over several reads: (0,"\e[2K"), (40,"  ⎿  API Error: 529 Overloaded\r\n"), (80,"\e[2K> \r\n"), (120,"│ >  │\r\n"), until 5000 → Error — :75-77
- Burst(0,10), until 5000 → Done — :78
- Burst(0,10)+Dialog(1000)+Burst(5100,4), key (5000,"\e[B") (arrow), until 9000 → NeedsInput — :81-82
- Same with key (5000,"\e[I") (focus report) → NeedsInput — :83-84
- Burst(0,10)+Dialog(1000)+Burst(5100,10), key (5000,"\r"), until 5600 → Working — :87-88
- Same, until 9000 → Done — :89-90
- Burst(0,10)+Dialog(1000)+(5100,"  ⎿  Interrupted\r\n"), key (5000,"\e") (bare ESC answers), until 9000 → Done — :91-92
- Burst(0,10)+Dialog(1000)+Burst(5100,10)+Dialog(6000), key (5000,"1"), until 12000 → NeedsInput — :93-94
- Typing: 30 keys "a" at 6000+i·120, echo "\e[2Ka" at key+30. Burst(0,10)+echo, until 8000 → Done (echo within 250 ms of a key is not work) — :97-101
- Same + Burst(10500,10), keys + (10200,"\r"), until 11000 → Working — :102-103
- from_turn(NeedsInput, Working, asked+3 s, asked, answered=false) → None — :108-109
- from_turn(NeedsInput, Working, asked+3 s, asked, answered=true) → Some(Working) — :110-111
- from_turn(NeedsInput, Done, asked+9 s, asked, answered=true) → Some(Done) — :112-113
- Implied unit tests worth adding: answers(b"\x1b[A") == false, answers(b"\x1b") == true, answers(b"\r") == true; counts_as_work thresholds (>500 ms after a resize AND >250 ms after a key); scan carry across the split "Do you w"/"ant to"; the ESC[1C split across reads; codex-only "Would you like to"; from_timer(Exited…) is not reached (the tick skips Exited) — TP:387-388, 497-531
