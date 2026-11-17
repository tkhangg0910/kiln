# PORTING-LINUX.md: Kilno trên Linux

Kế hoạch đưa Kilno (hiện chỉ chạy Windows 10/11 x64) lên Linux x86_64. Viết theo kiểu `PORTING.md`:
mỗi việc có một mã `LNX-…` và một ô `[ ]`. Một giai đoạn chỉ xong khi mọi mã của nó được tick `[x]`,
hoặc `[~]` (tạm hoãn đã duyệt, ghi lý do ở §7).

Phạm vi: bản Windows giữ nguyên hành vi. Tài liệu này không đổi cách Kilno chạy trên Windows, chỉ tách
phần Windows ra sau một lớp `platform` để Linux có bản cài đặt của riêng mình.

---

## 1. Hiện trạng (đo trên `main` @ 0.2.0)

| Crate | Dòng | Phụ thuộc Windows | Ghi chú |
|---|---|---|---|
| `kilno-term` | ~1.500 | không | `alacritty_terminal` + egui, chạy được ngay |
| `kilno-core` | ~8.300 | `winreg` (đã gate `cfg(windows)`) | Logic thuần. Còn giả định Windows về đường dẫn, `PATH`, shell |
| `kilno-pty` | ~340 | ConPTY (`sys.rs`, 283 dòng) | Trên non-Windows `spawn` trả `Unsupported` |
| `kilno-island` | ~4.200 | Win32 layered window (`sys/win.rs`, 658 dòng) | Phần vẽ (`tiny-skia`, anim, mascot) dùng lại được |
| `kilno-app` | ~7.900 | `windows`, `rfd` (đã gate) | `win/` (instance, tray, http, shell) + ~35 chỗ `#[cfg(windows)]` |

Đã thuận lợi:
- Dependency Windows đều nằm dưới `[target.'cfg(windows)'.dependencies]`.
- `kilno-core` đọc biến môi trường qua trait `Env`, nên test đổi được môi trường dễ dàng.
- eframe/egui, `alacritty_terminal`, `notify`, `tiny-skia` vốn đa nền tảng.

Giả định Windows còn nằm trong code dùng chung (phải sửa, không chỉ gate):

| Chỗ | Giả định |
|---|---|
| `kilno-core::paths` | `LOCALAPPDATA`, `USERPROFILE` |
| `kilno-core::launch` | `resolve_shell` chọn `pwsh.exe`/`powershell.exe`; `find_on_path` tách `PATH` bằng `;` |
| `kilno-pty::PtyConfig` | `command_line: String` là chuỗi kiểu `CreateProcessW`, không phải argv |
| `kilno-core::agent::detect` | Tên `claude.exe`, `claude.cmd`, `npm.cmd`, `%ProgramFiles%\nodejs`; kill cây tiến trình kiểu Windows |
| Status line | Lệnh ghi vào settings của Claude là `kilno.exe --statusline`, có quoting kiểu Windows |
| `kilno-app::pane` | Gửi `\x03` để Claude tự thoát. Trên PTY Unix vẫn đúng, nhưng kill dùng `SIGTERM`/`SIGKILL` |
| wgpu | Feature `dx12` là fallback duy nhất |

## 2. Mục tiêu và ngoài phạm vi

Mục tiêu:
- Ubuntu 22.04+, Fedora 39+, Arch; X11 và Wayland (GNOME, KDE).
- Cùng tính năng cốt lõi: pane Claude/Codex, sidebar, status, quota Fable, settings, security rules, updater.

Ngoài phạm vi (lần này):
- macOS. Nhưng lớp `platform` nên viết để sau này thêm được.
- ARM64.
- Gói `.deb`/`.rpm`/Flatpak. Lần đầu chỉ phát hành tar.gz (và AppImage nếu kịp).

## 3. Kiến trúc đích

```
kilno-core
 └─ platform/           trait + hàm theo OS, chọn bằng cfg
     ├─ mod.rs          Paths, Shell, ProcessTree, LoginStart, UserPath
     ├─ windows.rs      code hiện có, chuyển vào đây
     └─ unix.rs         bản Linux
kilno-pty
 ├─ sys.rs              ConPTY (giữ nguyên, đổi tên sys/windows.rs)
 └─ sys/unix.rs         openpty + fork/exec (rustix hoặc portable-pty)
kilno-island
 └─ sys/{win,x11,wayland}.rs   cùng một API "show surface ở rect, alpha, click-through"
kilno-app
 └─ os/{win,unix}/      instance, tray, http, shell (đổi tên `win/` → `os/win/`)
```

Quy tắc:
- Code chung không được có `#[cfg(windows)]` lẻ tẻ. Gom về một hàm trong `platform`/`os`, gọi chung một tên.
- Mỗi API platform có test chạy được trên cả hai OS (dùng `Env`/thư mục tạm như hiện tại).
- `unsafe` trên Linux theo cùng luật: `// SAFETY` bắt buộc, chỉ nằm trong `sys`.

## 4. Bảng ánh xạ

| Việc | Windows (hiện tại) | Linux (đề xuất) |
|---|---|---|
| Thư mục dữ liệu | `%LOCALAPPDATA%\Kilno` | `$XDG_DATA_HOME/kilno` (mặc định `~/.local/share/kilno`); settings vẫn ở đó cho đơn giản |
| Home của agent | `%USERPROFILE%\.claude`, `.codex`, `.claude.json` | `$HOME/...`; vẫn tôn trọng `CLAUDE_CONFIG_DIR`, `CODEX_HOME` |
| Shell của pane | `pwsh` → `powershell` | `$SHELL` → `/bin/bash` → `/bin/sh`, chạy `-l` để có PATH của user |
| PTY | ConPTY | `openpty`, `setsid`, `TIOCSCTTY`, `TIOCSWINSZ`; đọc master trên thread riêng |
| Kết thúc pane | `TerminateProcess` + đóng ConPTY | `SIGHUP` cho process group → chờ → `SIGKILL` |
| Một instance | Named mutex + named pipe | `flock` trên `$XDG_RUNTIME_DIR/kilno.lock` + Unix socket cùng chỗ |
| Đưa cửa sổ lên trước | `AllowSetForegroundWindow` | Gửi lệnh qua socket rồi `ViewportCommand::Focus`. Wayland có thể chỉ nhấp nháy taskbar |
| Icon khay | `Shell_NotifyIconW` | `ksni` (StatusNotifierItem qua D-Bus). GNOME cần extension AppIndicator; không có thì bỏ qua êm |
| HTTP (updater) | WinHTTP | `ureq` + `rustls` (không phụ thuộc OpenSSL) |
| Mở file/thư mục/link | `ShellExecuteW` | `xdg-open` (spawn, không chờ) |
| Hộp chọn thư mục | `rfd` Win32 | `rfd` feature `xdg-portal` (không kéo GTK) |
| Tự chạy khi đăng nhập | `HKCU\...\Run` | `~/.config/autostart/kilno.desktop` với `Exec=... --background` |
| User PATH | Registry `Environment\Path` | Không sửa shell rc. Cài vào `~/.local/bin`, báo user nếu thư mục đó chưa có trong PATH |
| Tìm agent | `claude.exe`/`.cmd`, `npm.cmd` | `claude`, `codex`, `npm` trên PATH (tách `:`), `~/.local/bin`, `~/.npm-global/bin`, `$(npm prefix -g)/bin` |
| Ẩn cửa sổ console con | `CREATE_NO_WINDOW` | Không cần |
| Renderer | glow, fallback wgpu dx12 | glow (EGL/GLX), fallback wgpu `vulkan` + `gles` |
| Island | Layered window, topmost, no-activate | X11: override-redirect ARGB + XShape input. Wayland: `wlr-layer-shell` (KDE, Sway, Hyprland). GNOME Wayland: không có, xem §5.3 |
| Phím tắt chụp màn hình | `RegisterHotKey` Ctrl+Alt+S | X11: `XGrabKey`. Wayland: portal `GlobalShortcuts` nếu có, không thì tắt |
| Đọc ảnh clipboard | Win32 clipboard | `arboard` (X11 + Wayland data-control) |
| Thư mục ảnh chụp | `Pictures\Screenshots` | `$(xdg-user-dir PICTURES)/Screenshots` |
| Crash log | `%LOCALAPPDATA%\Kilno\crash.log` | `<data>/crash.log` |
| Gói phát hành | `kilno-x.y.z-win-x64.zip` | `kilno-x.y.z-linux-x64.tar.gz` (+ `.AppImage`) |
| Cài đặt | `scripts/install.ps1` | `scripts/install.sh`: tải, kiểm sha256, giải nén vào `~/.local/share/kilno/bin`, link `~/.local/bin/kilno`, tạo `.desktop` |
| Updater tự thay file | Đổi tên exe đang chạy | Ghi file mới cạnh file cũ rồi `rename` (Linux cho thay file đang chạy), khởi động lại |

## 5. Giai đoạn

### 5.0 Biên dịch được trên Linux (1 ngày)

- [ ] **LNX-001** CI thêm job `ubuntu-latest`: `cargo check --workspace --all-targets`.
- [ ] **LNX-002** `cargo test -p kilno-core -p kilno-term` xanh trên Linux. Test nào cứng đường dẫn Windows thì chuyển qua `Env` hoặc gate có lý do.
- [ ] **LNX-003** `kilno-app` biên dịch được với stub: cửa sổ mở, pane báo "PTY chưa hỗ trợ".
- [ ] **LNX-004** wgpu: `dx12` chỉ cho Windows, `vulkan`+`gles` cho Linux (target-specific features).

### 5.1 Lớp platform (2 ngày)

- [ ] **LNX-010** `kilno-core::platform::Paths`: `data_dir`, `home`, `claude_dir`, `codex_dir`, `claude_json`. `paths.rs` gọi qua đây; test cả hai bộ biến môi trường.
- [ ] **LNX-011** `find_on_path` dùng `std::env::split_paths` thay cho `split(';')`, và nhận tên file theo OS (`.exe` chỉ trên Windows).
- [ ] **LNX-012** `Shell`: `resolve_shell` trả argv (`Vec<OsString>`), không trả chuỗi. Windows vẫn ra `pwsh -NoLogo`.
- [ ] **LNX-013** `PtyConfig.command_line` → `program` + `args` (+ `env`). Bên Windows tự ghép thành command line như hiện tại; test so khớp byte với chuỗi cũ.
- [ ] **LNX-014** Cách gõ lệnh agent vào shell: quoting `sh` (`'…'`, thay `'` bằng `'\''`) bên cạnh quoting PowerShell. Có test với đường dẫn có dấu cách và ký tự Unicode.
- [ ] **LNX-015** `ProcessTree::kill`: Windows giữ nguyên; Unix kill cả process group (pane là session leader nhờ `setsid`).
- [ ] **LNX-016** Gom các `#[cfg(windows)]` trong `app.rs`, `settings_page.rs`, `setup_window.rs`, `island_host.rs`, `pages_open.rs`, `updater.rs` thành lời gọi `os::…`.

### 5.2 MVP Linux (3–4 ngày): dùng được hằng ngày, chưa có island

- [ ] **LNX-020** `kilno-pty/sys/unix.rs`: openpty, fork/exec với `setsid` và controlling tty, resize bằng `TIOCSWINSZ`, thread đọc gửi `PtyEvent::Output`, `waitpid` gửi `Exited`. Giữ đúng hợp đồng trong `lib.rs` (không có event sau khi drop).
- [ ] **LNX-021** Đóng pane: `\x03` hai lần cách 600 ms (giữ nguyên), rồi `SIGHUP` → `SIGKILL` sau `QUIT_GRACE`.
- [ ] **LNX-022** Đặt `TERM=xterm-256color`, `COLORTERM=truecolor` cho pane; xoá `INHERITED_*` như Windows.
- [ ] **LNX-023** Một instance: lock + socket. Lần chạy thứ hai gửi thư mục rồi thoát; lần đầu mở pane đúng thư mục đó.
- [ ] **LNX-024** Updater: `ureq`; asset `linux-x64.tar.gz`; kiểm sha256; thay file bằng `rename`; khởi động lại.
- [ ] **LNX-025** `xdg-open` cho file, thư mục, link, HTML page (pages giữ CSP như cũ).
- [ ] **LNX-026** Hộp chọn thư mục và binary agent qua `rfd` portal; không có portal thì ô nhập tay.
- [ ] **LNX-027** Setup window tìm `claude`/`codex` theo §4. Nút cài đặt chạy `npm i -g …` hoặc script cài chính thức của Claude Code. Không dùng `sudo`.
- [ ] **LNX-028** Status line: ghi lệnh `'<đường dẫn kilno>' --statusline [--pass]` với quoting `sh`. Đọc lại đúng settings Claude đã có. Test round-trip.
- [ ] **LNX-029** Tự chạy khi đăng nhập qua file `.desktop` autostart; file đánh dấu `no-login-start` giữ nguyên ý nghĩa.
- [ ] **LNX-030** Font: bundle sẵn font như Windows. Nếu có dùng font hệ thống thì fallback `fontconfig` (DejaVu/Noto) để không bị ô vuông.
- [ ] **LNX-031** Icon ứng dụng + `kilno.desktop` (Name, Exec, Icon, `StartupWMClass` khớp app id của eframe).

### 5.3 Island (4–6 ngày)

Island trên Linux là phần rủi ro nhất. Code vẽ (`view`, `glow`, `mascot`, `anim`, `dib`) giữ nguyên; chỉ phần "đưa bitmap lên màn hình" là theo OS.

- [ ] **LNX-040** Tách `kilno-island::sys` thành trait `Surface { show(rect), set_pixels(&[u8]), set_input_region(rect|none), hide() }`. `win.rs` chỉ là một bản cài đặt.
- [ ] **LNX-041** X11: cửa sổ override-redirect, visual ARGB32, `_NET_WM_STATE_ABOVE`, input shape rỗng khi chỉ hiện glow; dùng `x11rb`.
- [ ] **LNX-042** Wayland có `zwlr_layer_shell_v1` (KDE, Sway, Hyprland): layer `overlay`, anchor top, `keyboard_interactivity = none`; dùng `smithay-client-toolkit`.
- [ ] **LNX-043** GNOME Wayland (không có layer-shell): island đổi sang thông báo hệ thống (`org.freedesktop.Notifications`) + chấm báo trên sidebar. Ghi rõ trong docs.
- [ ] **LNX-044** Kéo thả file vào island (drop-to-ask): X11 XDND; Wayland `wl_data_device`. Nếu không kịp thì `[~]`, vẫn còn kéo thả vào cửa sổ chính.
- [ ] **LNX-045** HiDPI: scale theo `Xft.dpi` (X11) và `wl_output.scale`/fractional-scale (Wayland).
- [ ] **LNX-046** Nhiều màn hình: island chọn màn hình chứa cửa sổ Kilno, như Windows.

### 5.4 Khay và phím tắt (2 ngày)

- [ ] **LNX-050** Icon khay bằng `ksni`: click trái hiện cửa sổ, menu Show Kilno / Quit Kilno. Không có StatusNotifierWatcher thì không lỗi, chỉ ghi log.
- [ ] **LNX-051** Ctrl+Alt+S: X11 `XGrabKey`. Wayland: portal `GlobalShortcuts` (KDE ≥ 5.27, GNOME ≥ 48). Không có thì Settings › Island ghi "not available on this desktop" và bật gợi ý dùng "Offer every new screenshot".
- [ ] **LNX-052** Theo dõi thư mục ảnh chụp mới, như Windows, đường dẫn theo `xdg-user-dir`.

### 5.5 Phát hành (1–2 ngày)

- [ ] **LNX-060** `release.yml` thêm job Linux, build trên `ubuntu-22.04` để glibc đủ cũ (chạy được trên distro mới hơn).
- [ ] **LNX-061** Asset: `kilno-x.y.z-linux-x64.tar.gz` + `.sha256`; AppImage nếu kịp.
- [ ] **LNX-062** `scripts/install.sh` (kiểm sha256, không sudo, không sửa shell rc).
- [ ] **LNX-063** Updater chọn asset theo OS; bản Windows cũ (0.2.x) bỏ qua asset Linux. Có test với JSON release mẫu chứa cả hai asset.
- [ ] **LNX-064** Docs: `installation.md`, `troubleshooting.md` (Wayland, GNOME tray, portal), `faq.md` ("macOS or Linux?"), `architecture.md` (§Process model, Rendering), README.

## 6. Kiểm thử

| Mức | Cách |
|---|---|
| Unit | `cargo test` trên Windows và Ubuntu trong CI; logic platform test bằng `Env`/thư mục tạm |
| PTY | Test tích hợp: spawn `sh -c 'printf hi; exit 3'` → nhận `hi` và `Exited(Some(3))`; resize; kill cả group (không còn tiến trình con mồ côi) |
| Clippy | `-D warnings` trên cả hai OS |
| Thủ công | WSL2 + WSLg trên máy dev (X11 qua XWayland, Wayland Weston): đủ cho MVP |
| Island, tray, phím tắt | Máy thật hoặc VM: Ubuntu 24.04 GNOME (Wayland + X11), Fedora KDE (Wayland), một máy Sway/Hyprland |
| Agent | Claude Code và Codex cài qua npm và qua script cài chính thức; cả `CLAUDE_CONFIG_DIR` tuỳ chỉnh |

Checklist chạy tay trước khi phát hành:
1. Cài bằng `install.sh` trên máy sạch, mở từ menu ứng dụng.
2. New session Claude, có màu, gõ mượt, resize cửa sổ, đóng pane: Claude tự thoát, không còn tiến trình `claude`/`node` thừa.
3. Resume, fork, đổi tên, quota 5 giờ/7 ngày/Fable hiện đúng.
4. Đóng cửa sổ → icon khay (hoặc island) đưa lại được; Quit dọn sạch.
5. Mở `kilno /some/dir` lần hai → instance đầu mở đúng thư mục.
6. Updater thấy bản mới hơn, tải, kiểm checksum, khởi động lại.

## 7. Rủi ro và tạm hoãn dự kiến

| Mã | Rủi ro | Hướng xử lý |
|---|---|---|
| R1 | GNOME Wayland không cho cửa sổ luôn nằm trên, không đặt được vị trí | LNX-043: thông báo hệ thống thay island. Chấp nhận khác Windows |
| R2 | GNOME không có tray mặc định | Không coi là lỗi. Cửa sổ ẩn vẫn gọi lại được bằng cách chạy `kilno` lần nữa (instance socket) |
| R3 | Phím tắt toàn hệ thống trên Wayland | Portal nếu có; không thì `[~]` với "Offer every new screenshot" |
| R4 | Driver GL lỗi (VM, NVIDIA cũ) | Fallback wgpu Vulkan, rồi `gles`; biến `KILNO_RENDERER=wgpu` để ép |
| R5 | glibc quá mới trên máy build | Build trên `ubuntu-22.04`; cân nhắc target musl cho bản tĩnh |
| R6 | Claude Code trên Linux đổi chỗ cache (`.claude.json`) hoặc tên binary | Đường dẫn đi qua `platform::Paths` + test; quota Fable thiếu thì ẩn, như Windows |
| R7 | Code platform rẽ nhánh làm hỏng Windows | CI chạy cả hai OS; LNX-013 có test so khớp command line Windows byte-by-byte |

## 8. Ước lượng

| Giai đoạn | Thời gian | Kết quả |
|---|---|---|
| 5.0 + 5.1 | ~3 ngày | Biên dịch và test trên Linux, Windows không đổi hành vi |
| 5.2 | ~4 ngày | **MVP**: dùng hằng ngày được, phát hành thử `0.3.0-linux-preview` |
| 5.3 | ~5 ngày | Island trên X11 và KDE/Sway; GNOME Wayland dùng thông báo |
| 5.4 + 5.5 | ~3 ngày | Tray, phím tắt, release chính thức `0.3.0` |
| **Tổng** | **~2,5–3 tuần** | |

Thứ tự làm: 5.0 → 5.1 → 5.2 phát hành preview → 5.3/5.4 song song → 5.5. Nhánh: `0.3-linux`, mỗi giai đoạn
một PR vào nhánh đó; merge vào `main` khi 5.2 xong và CI Windows vẫn xanh.
