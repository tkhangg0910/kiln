"""Records the Kilno demo clips described in docs/demo/actions.yaml.

Run inside .launchkit/venv:
  python scripts/launchkit/record.py              # all scenes, filmed
  python scripts/launchkit/record.py --dry-run    # all scenes, not filmed
  python scripts/launchkit/record.py --scene 03-island   # replays earlier scenes unfilmed, films this one
  python scripts/launchkit/record.py --screenshots       # runs the scenes and takes the README shots
  python scripts/launchkit/record.py --inspect           # prints the UIA names of the Kilno window

Kilno runs fully isolated (demo-data/env as its Claude, Codex and LocalAppData folders, USERPROFILE =
D:\\dev). Each clip is docs/demo/raw/<scene>.mp4 (work area, 60 fps, H.264 crf 16 preset slow) plus
<scene>.events.json (t_ms from the first frame, type, x, y in video pixels, target, zoom). The mouse
pointer is not captured; the editor redraws it from the move events.
"""
import argparse, ctypes, json, math, os, random, re, shutil, subprocess, sys, threading, time
from datetime import datetime

ctypes.windll.shcore.SetProcessDpiAwareness(2)  # physical pixels everywhere
sys.stdout.reconfigure(encoding="utf-8")

import yaml
import win32api, win32con, win32gui
from pywinauto import Application, Desktop, keyboard, mouse

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
D = os.path.join(REPO, "demo-data")
RAW = os.path.join(REPO, "docs", "demo", "raw")
ASSETS = os.path.join(REPO, "docs", "assets")
LOG = os.path.join(REPO, ".launchkit", "LOG.md")
KILNO = os.path.join(REPO, "target", "debug", "kilno.exe")
EDGE = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
SCREEN_W = win32api.GetSystemMetrics(0)


def log(msg):
    line = f"- [{datetime.now():%H:%M:%S}] {msg}"
    print(line, flush=True)
    with open(LOG, "a", encoding="utf-8") as f:
        f.write(line + "\n")


class StepError(Exception):
    pass


class Recorder:
    """ffmpeg gdigrab of the region; t0 = the moment ffmpeg starts delivering frames."""

    def __init__(self, scene, region, film):
        self.scene, self.region, self.film = scene, region, film
        self.events, self.proc, self.t0 = [], None, time.perf_counter()

    def start(self):
        if not self.film:
            self.t0 = time.perf_counter()
            return
        x, y, w, h = self.region
        self.tmp = os.path.join(RAW, f"{self.scene}.lossless.mkv")
        self.proc = subprocess.Popen(
            ["ffmpeg", "-hide_banner", "-loglevel", "info", "-y", "-f", "gdigrab", "-framerate", "60",
             "-draw_mouse", "0", "-offset_x", str(x), "-offset_y", str(y), "-video_size", f"{w}x{h}",
             "-i", "desktop", "-c:v", "libx264rgb", "-preset", "ultrafast", "-qp", "0", self.tmp],
            stdin=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        started = threading.Event()

        def drain():  # keep reading, or ffmpeg blocks on a full stderr pipe
            for line in self.proc.stderr:
                if "Press [q]" in line or "frame=" in line:
                    started.set()
            started.set()

        threading.Thread(target=drain, daemon=True).start()
        started.wait(15)
        self.t0 = time.perf_counter()

    def ev(self, type_, x, y, target="", zoom=None, **extra):
        e = {"t_ms": round((time.perf_counter() - self.t0) * 1000), "type": type_,
             "x": int(x - self.region[0]), "y": int(y - self.region[1]), "target": target}
        if zoom:
            e["zoom"] = zoom
        e.update(extra)
        self.events.append(e)

    def stop(self):
        if not self.film:
            return
        dur = round((time.perf_counter() - self.t0) * 1000)
        self.proc.stdin.write("q")
        self.proc.stdin.flush()
        self.proc.wait(30)
        out = os.path.join(RAW, f"{self.scene}.mp4")
        subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", self.tmp, "-c:v", "libx264",
                        "-preset", "slow", "-crf", "16", "-pix_fmt", "yuv420p", "-r", "60", out], check=True)
        os.remove(self.tmp)
        with open(os.path.join(RAW, f"{self.scene}.events.json"), "w", encoding="utf-8") as f:
            json.dump({"scene": self.scene, "region": self.region, "duration_ms": dur, "events": self.events}, f, indent=1)


class Demo:
    def __init__(self, cfg, film, screenshots):
        self.cfg, self.film, self.screenshots = cfg, film, screenshots
        self.sessions = json.load(open(os.path.join(D, "sessions.json")))
        self.kilno = self.app = self.win = None
        self.edge = self.edge_win = None
        self.consoles = []
        self.rec = Recorder("none", cfg["common"]["region"], False)
        self.env = dict(os.environ,
                        CLAUDE_CONFIG_DIR=os.path.join(D, "env", "claude"), CODEX_HOME=os.path.join(D, "env", "codex"),
                        LOCALAPPDATA=os.path.join(D, "env", "local"), CLAUDE_CODE_TMPDIR=os.path.join(D, "env", "tmp"),
                        USERPROFILE=r"D:\dev", HOMEDRIVE="D:", HOMEPATH=r"\dev",
                        KILNO_DEV_AGENT_CMD="demo-agent {id}", KILNO_DEMO_EXE=KILNO,
                        KILNO_DEMO_ROOT=os.path.join(D, "agent"),
                        PATH=os.path.join(D, "bin") + ";" + os.environ["PATH"])

    # ---------- windows ----------
    def launch_kilno(self, size):
        if self.kilno and self.kilno.poll() is None:
            return
        self.kilno = subprocess.Popen([KILNO], cwd=os.path.dirname(KILNO), env=self.env)
        self.app = Application(backend="uia").connect(process=self.kilno.pid, timeout=20)
        spec = self.app.window(title=self.cfg["common"]["window_title"])
        spec.wait("visible", timeout=20)
        self.win = spec.wrapper_object()  # bound to the HWND, found once
        time.sleep(1.0)
        self.win.maximize()
        self.focus_kilno()

    def focus_kilno(self):
        self.win.restore() if self.win.is_minimized() else None
        self.win.set_focus()
        win32gui.ShowWindow(self.win.handle, win32con.SW_MAXIMIZE)
        time.sleep(0.4)

    def client_origin(self):
        return win32gui.ClientToScreen(self.win.handle, (0, 0))

    def browser(self, url, title="Atlas docs"):
        if title != "Atlas docs" or not self.edge or self.edge.poll() is not None:
            prof = os.path.join(REPO, ".launchkit", "edge-profile")
            page = "file:///" + os.path.join(D, url).replace("\\", "/")
            proc = subprocess.Popen([EDGE, f"--app={page}", f"--user-data-dir={prof}", "--no-first-run",
                                          "--no-default-browser-check", "--disable-features=msEdgeSidebarV2",
                                          "--window-position=0,0", "--window-size=1920,1008"])
            if not self.edge or self.edge.poll() is not None:
                self.edge = proc
            time.sleep(4)
        self.edge_win = None
        for _ in range(20):
            hs = []
            win32gui.EnumWindows(lambda h, a: a.append(h) if title in win32gui.GetWindowText(h) and win32gui.IsWindowVisible(h) else None, hs)
            if hs:
                self.edge_win = hs[0]
                break
            time.sleep(0.5)
        if not self.edge_win:
            raise StepError("browser window not found")
        win32gui.ShowWindow(self.edge_win, win32con.SW_MAXIMIZE)
        self.force_front(self.edge_win)
        time.sleep(0.6)

    def force_front(self, h):
        # Alt tap lets this process move the foreground (SetForegroundWindow rules).
        win32api.keybd_event(0x12, 0, 0, 0)
        win32api.keybd_event(0x12, 0, 2, 0)
        try:
            win32gui.SetForegroundWindow(h)
        except Exception:
            pass

    def terminals(self, count, projects, ask_after):
        self.browser("site/backdrop.html", title="Desk")  # nothing of the desktop shows between tiles
        w, h = 960, 504
        for i in range(count):
            x, y = (i % 2) * w, (i // 2) * h
            p = subprocess.Popen(["conhost.exe", "cmd", "/c", f"title {projects[i]} & demo-agent --plain {projects[i]} {ask_after[i]}"],
                                 cwd=os.path.join(r"D:\dev", projects[i]), env=self.env)
            self.consoles.append(p)
            time.sleep(0.8)
            hs = []
            win32gui.EnumWindows(lambda hh, a: a.append(hh) if win32gui.GetWindowText(hh).startswith(projects[i]) else None, hs)
            for hh in hs:
                # Cover the 7 px invisible borders so the tiles meet.
                win32gui.SetWindowPos(hh, win32con.HWND_TOP, x - 7, y, w + 14, h + 7, 0x0040)

    def close_terminals(self):
        for p in self.consoles:
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], capture_output=True)
        self.consoles = []

    def clipboard_image(self):
        png = os.path.join(REPO, ".launchkit", "clip.png")
        from PIL import ImageGrab
        ImageGrab.grab(bbox=(240, 120, 1680, 900), all_screens=False).save(png)
        ps = ("Add-Type -AssemblyName System.Windows.Forms,System.Drawing;"
              f"[Windows.Forms.Clipboard]::SetImage([Drawing.Image]::FromFile('{png}'))")
        subprocess.run(["powershell", "-NoProfile", "-STA", "-Command", ps], check=True)

    # ---------- targets ----------
    def resolve(self, target):
        kind, _, arg = target.partition(":")
        if kind == "point":
            x, y = json.loads(arg)
            ox, oy = self.client_origin()
            return ox + x, oy + y
        if kind == "screen":
            return tuple(json.loads(arg))
        if kind == "island":
            dx, dy = json.loads(arg)
            return SCREEN_W // 2 + dx, dy
        if kind == "name":
            pat, _, ctype = arg.partition(":")
            rx = re.compile(pat)
            for c in self.win.descendants():
                if ctype and c.element_info.control_type != ctype:
                    continue
                if rx.search(c.window_text() or ""):
                    r = c.rectangle()
                    return (r.left + r.right) // 2, (r.top + r.bottom) // 2
            raise StepError(f"element not found: {target}")
        if kind == "menu":
            return self.menu_entry(arg)
        raise StepError(f"bad target {target}")

    def menu_entry(self, label):
        """An entry of the New session folder menu (an egui popup with no AccessKit names): found
        by reading the popup's pixels is overkill; entries are laid out from the popup top, so the
        Nth recent folder is at a fixed offset. atlas-api is the only recent folder here (first)."""
        for c in self.win.descendants():
            if (c.window_text() or "").startswith(label):
                r = c.rectangle()
                return (r.left + r.right) // 2, (r.top + r.bottom) // 2
        ox, oy = self.client_origin()
        return ox + 230, oy + 560

    # ---------- motion ----------
    def move(self, target, zoom=None):
        x1, y1 = self.resolve(target)
        x0, y0 = win32api.GetCursorPos()
        dist = math.hypot(x1 - x0, y1 - y0)
        dur = min(0.6, max(0.3, dist / 2400))
        n = max(2, int(dur * 60))
        for i in range(1, n + 1):
            t = i / n
            e = t * t * (3 - 2 * t)  # ease-in-out
            x, y = x0 + (x1 - x0) * e, y0 + (y1 - y0) * e
            win32api.SetCursorPos((int(x), int(y)))
            self.rec.ev("move", x, y)
            time.sleep(dur / n)
        if zoom:
            self.rec.ev("move", x1, y1, target, zoom, focus=True)
        return x1, y1

    def click(self, target, zoom=None, double=False):
        x, y = self.move(target)
        time.sleep(random.uniform(0.3, 0.5))
        self.rec.ev("click", x, y, target, zoom)
        (mouse.double_click if double else mouse.click)(coords=(x, y))

    def type(self, text, cps):
        for ch in text:
            if ch == "\n":
                keyboard.send_keys("{ENTER}")
            else:
                keyboard.send_keys(re.sub(r"([{}+^%~()])", r"{\1}", ch), with_spaces=True, pause=0)
            x, y = win32api.GetCursorPos()
            self.rec.ev("type", x, y, ch)
            time.sleep(max(0.02, random.gauss(1 / cps, 0.015)))

    # ---------- steps ----------
    def run_steps(self, steps, scene):
        for st in steps:
            (op, arg), = st.items()
            if op == "launch":
                self.launch_kilno(arg.get("size"))
            elif op == "terminals":
                self.terminals(arg["count"], arg["projects"], arg["ask_after"])
            elif op == "browser":
                self.browser(arg["url"])
            elif op == "focus":
                self.focus_kilno()
            elif op == "clipboard_image":
                if arg and arg.get("label"):
                    x, y = self.resolve(arg["at"])
                    self.rec.ev("key", x, y, arg["label"], arg.get("zoom"))
                self.clipboard_image()
            elif op == "click_silent":
                self.click_silent(arg)
                time.sleep(0.6)
            elif op == "clear_search":
                self.click_silent("name:^$:Edit")
                keyboard.send_keys("^a{BACKSPACE}")
                time.sleep(0.4)
            elif op == "open_row":
                self.open_row(arg["session"])
            elif op in ("wait", "hold"):
                if op == "hold":
                    x, y = win32api.GetCursorPos()
                    self.rec.ev("hold", x, y, "", None, ms=int(arg * 1000))
                time.sleep(float(arg))
            elif op == "move":
                self.move(arg["target"], arg.get("zoom"))
            elif op in ("click", "double_click"):
                self.click(arg["target"], arg.get("zoom"), double=op == "double_click")
            elif op == "type":
                self.type(arg["text"], arg.get("cps", self.cfg["common"]["cps"]))
            elif op == "keys":
                x, y = self.resolve(arg["at"]) if arg.get("at") else win32api.GetCursorPos()
                self.rec.ev("key", x, y, arg.get("label", arg["keys"]), arg.get("zoom"))
                keyboard.send_keys(arg["keys"])
            elif op == "scroll":
                x, y = self.move(arg["target"])
                self.rec.ev("scroll", x, y, arg["target"], None, amount=arg["amount"])
                mouse.scroll(coords=(x, y), wheel_dist=arg["amount"])
            elif op == "agent":
                sid = self.sessions[arg["session"]]
                with open(os.path.join(D, "agent", "ctl", f"{sid}.txt"), "w", encoding="utf-8") as f:
                    f.write(f"{arg['do']} {time.time()}")
            elif op == "shot":
                if self.screenshots:
                    self.shot(arg)
            else:
                raise StepError(f"unknown step {op}")

    def click_silent(self, target):
        x, y = self.resolve(target)
        mouse.click(coords=(x, y))

    def open_row(self, session):
        """Activate a session's pane again: it is the first row under Open after a search reset."""
        self.focus_kilno()

    def shot(self, arg):
        from PIL import ImageGrab
        if arg["region"] == "window":
            r = self.win.rectangle()
            box = (max(0, r.left), max(0, r.top), min(SCREEN_W, r.right), min(1008, r.bottom))
        else:
            x, y, w, h = arg["region"]
            box = (x, y, x + w, y + h)
        time.sleep(0.4)
        ImageGrab.grab(bbox=box).save(os.path.join(ASSETS, arg["file"]))
        log(f"screenshot {arg['file']} {box}")

    def scene(self, sc, film):
        name = sc["scene"]
        self.run_steps(sc.get("setup", []), name)
        self.rec = Recorder(name, self.cfg["common"]["region"], film)
        if film:
            print("\n" + "=" * 60 + "\n  ĐANG QUAY – không chạm chuột/bàn phím\n" + "=" * 60, flush=True)
            for i in range(5, 0, -1):
                print(f"  {i}…", flush=True)
                time.sleep(1)
        self.rec.start()
        try:
            self.run_steps(sc.get("steps", []), name)
        finally:
            self.rec.stop()
        if film:
            print("  QUAY XONG\n", flush=True)
        if name == "01-problem":
            self.close_terminals()
            if self.edge and self.edge.poll() is None:
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(self.edge.pid)], capture_output=True)
                self.edge = None

    def close(self):
        self.close_terminals()
        for p in (self.kilno, self.edge):
            if p and p.poll() is None:
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], capture_output=True)


def minimize_others():
    """Shell-minimize every other window (Win+D style) so nothing private shows behind."""
    Desktop(backend="uia")  # warm up COM
    subprocess.run(["powershell", "-NoProfile", "-Command",
                    "(New-Object -ComObject Shell.Application).MinimizeAll()"], check=False)
    time.sleep(1.0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--scene")
    ap.add_argument("--screenshots", action="store_true")
    ap.add_argument("--inspect", action="store_true")
    ap.add_argument("--only", help="comma-separated scenes to run (others skipped entirely)")
    ap.add_argument("--from", dest="from_", help="film this scene and every later one (earlier ones replayed unfilmed)")
    a = ap.parse_args()
    cfg = yaml.safe_load(open(os.path.join(REPO, "docs", "demo", "actions.yaml"), encoding="utf-8"))
    os.makedirs(RAW, exist_ok=True)
    os.makedirs(ASSETS, exist_ok=True)
    subprocess.run([sys.executable, os.path.join(REPO, "scripts", "launchkit", "make_demo_data.py"), REPO], check=True)
    minimize_others()
    demo = Demo(cfg, film=not a.dry_run, screenshots=a.screenshots)
    try:
        if a.inspect:
            demo.launch_kilno(None)
            for c in demo.win.descendants():
                if c.window_text():
                    print(c.element_info.control_type, repr(c.window_text()[:80]), c.rectangle())
            return
        scenes = cfg["scenes"]
        only = set(a.only.split(",")) if a.only else None
        names = [sc["scene"] for sc in scenes]
        start = names.index(a.from_) if a.from_ else 0
        for idx, sc in enumerate(scenes):
            name = sc["scene"]
            if only and name not in only:
                continue
            film = (not a.dry_run and not a.screenshots) and (a.scene == name if a.scene else idx >= start)
            for attempt in range(1, 4):
                try:
                    demo.scene(sc, film)
                    log(f"{'filmed' if film else 'ran'} {name}" + (f" (attempt {attempt})" if attempt > 1 else ""))
                    break
                except Exception as e:  # StepError, pywinauto lookups, timeouts
                    log(f"{name}: {e} (attempt {attempt})")
                    if attempt == 3:
                        raise
            if a.scene == name:
                break
    finally:
        demo.close()


if __name__ == "__main__":
    main()
