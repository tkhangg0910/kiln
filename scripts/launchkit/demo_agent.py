"""Stand-in coding agent for the Kiln demo recordings (not part of Kiln).

A real demo would need a signed-in Claude Code account; this script plays an agent in a Kiln
pane instead, with its own neutral look. It:
  - prints the kind of progress an agent prints (in bursts, so Kiln sees work),
  - asks "Do you want to ...?" when it needs approval (Kiln's needs-you marker),
  - appends its transcript where Kiln reads Claude sessions (CLAUDE_CONFIG_DIR\\projects),
  - reports model / context / limits through Kiln's own relay (`kiln.exe --statusline`),
  - takes commands from a control file so the recorder can time each scene:
      demo-data/ctl/<session-id>.txt  ->  work | ask | finish | doc | idle
Keys typed in the pane are read too: a line ending in Enter is a new prompt, "1"/"2" answer a
question, and a bracketed paste of an image path becomes "[Image #n]".

Usage (typed by Kiln through KILN_DEV_AGENT_CMD):  demo-agent <session-id>
"""
import json, msvcrt, os, random, re, shutil, subprocess, sys, time
from datetime import datetime, timedelta, timezone

ROOT = os.environ.get("KILN_DEMO_ROOT", os.path.dirname(os.path.abspath(__file__)))
SCEN = json.load(open(os.path.join(ROOT, "scenarios.json"), encoding="utf-8"))
KILN = os.environ.get("KILN_DEMO_EXE", "")
IMG = re.compile(r"(?:\x1b\[200~)?[A-Za-z]:\\[^\r\n\x1b]*?\.(?:png|jpe?g|gif|webp|bmp)(?:\x1b\[201~)?", re.I)

E = "\x1b["
DIM, BOLD, RESET = E + "2m", E + "1m", E + "0m"
AMBER, GREEN, BLUE, GREY, RED = E + "38;2;212;162;76m", E + "38;2;107;191;138m", E + "38;2;90;169;230m", E + "38;2;122;130;143m", E + "38;2;210;96;95m"

sid = sys.argv[1] if len(sys.argv) > 1 else "plain"
plain = sid == "--plain"
cwd = os.getcwd()
project = os.path.basename(cwd.rstrip("\\/"))
sc = SCEN["sessions"].get(sid) or SCEN["projects"].get(project) or SCEN["projects"]["default"]
ctl = os.path.join(ROOT, "ctl", f"{sid}.txt")
claude_dir = os.environ.get("CLAUDE_CONFIG_DIR", "")
tfile = os.path.join(claude_dir, "projects", "D--dev-" + project, f"{sid}.jsonl") if claude_dir and not plain else None
images = 0
ctx_used = sc.get("context", 48000)


def out(s="", end="\r\n"):
    sys.stdout.write(s + end)
    sys.stdout.flush()


def now_iso():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z"


def transcript(obj):
    if not tfile:
        return
    os.makedirs(os.path.dirname(tfile), exist_ok=True)
    obj.setdefault("timestamp", now_iso())
    obj.setdefault("sessionId", sid)
    obj.setdefault("cwd", cwd)
    with open(tfile, "a", encoding="utf-8") as f:
        f.write(json.dumps(obj) + "\n")


def status():
    """One status-line report through Kiln's relay, as Claude Code's status line would send."""
    if not KILN or plain:
        return
    resets5 = int((datetime.now(timezone.utc) + timedelta(hours=2, minutes=14)).timestamp())
    resets7 = int((datetime.now(timezone.utc) + timedelta(days=3, hours=6)).timestamp())
    payload = {
        "session_id": sid,
        "cwd": cwd,
        "model": {"display_name": sc.get("model", "Opus 5.5")},
        "effort": {"level": sc.get("effort", "high")},
        "context_window": {"context_window_size": 200000,
                           "current_usage": {"input_tokens": 1200, "cache_creation_input_tokens": 4000,
                                             "cache_read_input_tokens": max(0, ctx_used - 5200)}},
        "rate_limits": {"five_hour": {"used_percentage": SCEN.get("five_hour", 38), "resets_at": resets5},
                        "seven_day": {"used_percentage": SCEN.get("seven_day", 21), "resets_at": resets7}},
    }
    try:
        subprocess.run([KILN, "--statusline"], input=json.dumps(payload).encode(), timeout=5,
                       creationflags=0x08000000)
    except Exception:
        pass


def type_out(text, cps=180):
    for ch in text:
        sys.stdout.write(ch)
        sys.stdout.flush()
        time.sleep(1 / cps)
    out()


def header():
    out(E + "2J" + E + "H", end="")
    out(f"{AMBER}◆{RESET} {BOLD}demo agent{RESET}  {GREY}·  {project}  ·  {sc.get('model', 'Opus 5.5')}{RESET}")
    out(f"{GREY}{'─' * 64}{RESET}")


def prompt_line(text):
    out(f"{BLUE}›{RESET} {text}")


def work(steps):
    global ctx_used
    for i, step in enumerate(steps):
        out(f"  {AMBER}●{RESET} {step}")
        for _ in range(random.randint(2, 4)):
            out(f"    {GREY}{random.choice(SCEN['noise'])}{RESET}")
            time.sleep(random.uniform(0.08, 0.16))
        ctx_used += random.randint(2500, 6000)
        if i % 2 == 0:
            status()
        time.sleep(random.uniform(0.15, 0.3))


def ask(q):
    out()
    out(f"  {AMBER}?{RESET} {BOLD}Do you want to {q}{RESET}")
    out(f"    {BLUE}> 1. Yes{RESET}")
    out(f"      2. No, tell the agent what to do instead")
    out()


def finish(lines):
    for l in lines:
        out(f"  {GREEN}✓{RESET} {l}")
        time.sleep(0.12)
    status()
    out()
    sys.stdout.write(f"{BLUE}›{RESET} ")
    sys.stdout.flush()


def write_doc():
    doc = sc.get("doc")
    if not doc:
        return
    path = os.path.join(cwd, doc["path"])
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(doc["text"])
    out(f"  {AMBER}●{RESET} Wrote {doc['path']}")
    transcript({"type": "assistant", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "name": "Write", "input": {"file_path": path, "content": "…"}}]}})
    for rel, text in sc.get("edits", {}).items():
        p = os.path.join(cwd, rel)
        with open(p, "a", encoding="utf-8", newline="\n") as f:
            f.write(text)


def plain_loop():
    """For the 'four terminals' shot: endless scrolling, then one unnoticed question."""
    import ctypes
    k = ctypes.windll.kernel32
    h = k.GetStdHandle(-11)
    mode = ctypes.c_uint32()
    if k.GetConsoleMode(h, ctypes.byref(mode)):
        k.SetConsoleMode(h, mode.value | 0x0004)  # ENABLE_VIRTUAL_TERMINAL_PROCESSING
    proj = sys.argv[2] if len(sys.argv) > 2 else "service"
    t_end = time.time() + float(sys.argv[3]) if len(sys.argv) > 3 else None
    asked = False
    out(f"{AMBER}◆{RESET} agent · {proj}")
    while True:
        out(f"  {AMBER}●{RESET} {random.choice(SCEN['noise'])}")
        time.sleep(random.uniform(0.05, 0.2))
        if t_end and time.time() > t_end and not asked:
            ask("run database migrations on staging?")
            asked = True
            while True:
                time.sleep(1)


def main():
    if plain:
        plain_loop()
    global images
    header()
    for p in sc.get("history", []):
        prompt_line(p)
        out(f"  {GREEN}✓{RESET} {GREY}done{RESET}")
    if sc.get("prompt"):
        prompt_line(sc["prompt"])
        transcript({"type": "user", "message": {"role": "user", "content": sc["prompt"]}})
        transcript({"type": "ai-title", "aiTitle": sc.get("title", sc["prompt"])})
    status()
    sys.stdout.write(f"{BLUE}›{RESET} " if not sc.get("prompt") else "")
    sys.stdout.flush()
    os.makedirs(os.path.dirname(ctl), exist_ok=True)
    line, last_cmd, asked = "", "", False
    while True:
        # Commands from the recorder.
        try:
            cmd = open(ctl, encoding="utf-8").read().strip()
        except OSError:
            cmd = ""
        if cmd and cmd != last_cmd:
            last_cmd = cmd
            verb = cmd.split()[0]
            if verb == "work":
                out()
                work(sc.get("steps", []))
            elif verb == "ask":
                ask(sc.get("question", "apply these changes?"))
                asked = True
            elif verb == "finish":
                work(sc.get("after", [])[:3])
                write_doc()
                finish(sc.get("done", ["Done"]))
            elif verb == "doc":
                write_doc()
        # Keys from the pane. A paste arrives as a burst of characters; collect the burst first so
        # an image path can be shown as a tag instead of being echoed (ConPTY strips the
        # bracketed-paste markers before they reach a console app like this one).
        burst = ""
        while msvcrt.kbhit():
            burst += msvcrt.getwch()
        m = IMG.search(burst)
        if m:
            path = m.group(0)
            images += 1
            tmp = os.environ.get("CLAUDE_CODE_TMPDIR") or os.environ.get("TEMP", "")
            d = os.path.join(tmp, "claude", "D--dev-" + project, sid, "images")
            os.makedirs(d, exist_ok=True)
            try:
                shutil.copyfile(path, os.path.join(d, f"{images}.png"))
            except OSError:
                pass
            tag = f"[Image #{images}] "
            line += tag
            sys.stdout.write(BLUE + tag + RESET)
            sys.stdout.flush()
            burst = burst[:m.start()] + burst[m.end():]
        for ch in burst:
            if ch == "\x1b":  # bracketed paste: ESC [200~ path ESC [201~
                seq = ""
                while not seq.endswith("[201~") and len(seq) < 600:
                    seq += msvcrt.getwch()
                path = seq[len("[200~"):-len("[201~")].strip() if seq.startswith("[200~") else ""
                if path.lower().endswith((".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp")):
                    images += 1
                    tmp = os.environ.get("CLAUDE_CODE_TMPDIR") or os.environ.get("TEMP", "")
                    d = os.path.join(tmp, "claude", "D--dev-" + project, sid, "images")
                    os.makedirs(d, exist_ok=True)
                    try:
                        shutil.copyfile(path, os.path.join(d, f"{images}.png"))
                    except OSError:
                        pass
                    tag = f"[Image #{images}] "
                    line += tag
                    sys.stdout.write(BLUE + tag + RESET)
                    sys.stdout.flush()
                continue
            if asked and ch in "12":
                asked = False
                # Replace the question block with the answer, as TUI agents do: the question
                # text must not stay on screen (a repaint would show it to Kiln again).
                sys.stdout.write(E + "5F" + E + "J")
                out(f"  {GREEN}✓{RESET} {GREY}{'Approved: ' if ch == '1' else 'Declined: '}{sc.get('question', 'apply these changes?')}{RESET}")
                if ch == "1":
                    work(sc.get("after", []))
                    write_doc()
                    finish(sc.get("done", ["Done"]))
                continue
            if ch in "\r\n":
                out()
                if line.strip():
                    transcript({"type": "user", "message": {"role": "user", "content": line.strip()}})
                    transcript({"type": "last-prompt", "lastPrompt": line.strip()})
                    if not sc.get("prompt"):
                        transcript({"type": "ai-title", "aiTitle": line.strip()})
                        sc["prompt"] = line.strip()
                    work(sc.get("steps", [])[:4])
                    finish(sc.get("done", ["Done"])[:2])
                line = ""
            elif ch == "\b":
                if line:
                    line = line[:-1]
                    sys.stdout.write("\b \b")
                    sys.stdout.flush()
            elif ch.isprintable():
                line += ch
                sys.stdout.write(ch)
                sys.stdout.flush()
        time.sleep(0.03)


if __name__ == "__main__":
    main()
