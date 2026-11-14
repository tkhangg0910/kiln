"""Builds demo-data/ for the Kilno launch-kit recordings: fictional projects (git repos with a
branch and an uncommitted diff), past sessions as Claude-style transcripts, scenarios for the
stand-in agent, an isolated Kilno data folder, and a docs page for the "other app" shots.

Nothing here touches the user's real configuration: Kilno is pointed at demo-data/env through
CLAUDE_CONFIG_DIR, CODEX_HOME and LOCALAPPDATA by record.py. Re-running rebuilds everything.
Usage: python make_demo_data.py <repo-root>
"""
import json, os, shutil, subprocess, sys, time
from datetime import datetime, timedelta, timezone

REPO = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else ".")
D = os.path.join(REPO, "demo-data")
PROJ = os.path.join(D, "projects")
ENV = os.path.join(D, "env")
LINK = r"D:\dev"  # short, neutral path shown in the app (a junction to demo-data\projects)
HERE = os.path.dirname(os.path.abspath(__file__))

S = {  # session ids (stable, so scenarios and actions can name them)
    "rate": "5b1f0c3e-8a42-4c1e-9d3a-2f6b7e8a9c01",
    "dark": "a3c9e2f1-6d84-4b7a-8e25-1c9d0f3b4a12",
    "sync": "7e2d4a6b-1f39-4c8e-a5b0-3d6f8e2c9b23",
    "hooks": "c4f8b1a9-2e57-4d3c-b6a1-9f0e7d2c5a34",
    "inv": "1d6e9b3c-4a28-4f7e-8c51-6b2a0e9d7f45",
    "price": "e9a2c7d4-5b16-4e8f-9a3c-0d7b1f6e2c56",
    "cron": "3f7b2e8a-9c41-4d6a-b2e5-8a1c0f4d9e67",
    "rn": "8c1e5f2b-7a93-4b0d-8f6e-2c4a9d1b3e78",
}

NOISE = [
    "read src/orders/routes.ts (214 lines)", "read src/middleware/index.ts", "grep \"limit\" src --type ts",
    "read package.json", "ran npm test -- orders (42 passed)", "read src/config/env.ts",
    "edit src/orders/limiter.ts (+38 −0)", "read tests/orders.spec.ts", "ran tsc --noEmit (0 errors)",
    "read docs/api.md", "read src/lib/redis.ts", "ran npm run lint (clean)",
]

SCEN = {
    "five_hour": 38, "seven_day": 21, "noise": NOISE,
    "sessions": {
        S["rate"]: {
            "title": "Add rate limiting to the orders API", "prompt": "Add rate limiting to the orders API (100 req/min per key)",
            "model": "Opus 5.5", "effort": "high", "context": 61000,
            "steps": ["Reading the orders routes", "Checking the Redis client", "Planning a token-bucket limiter",
                      "Writing src/orders/limiter.ts", "Wiring the middleware"],
            "question": "apply this edit to src/orders/routes.ts?",
            "after": ["Updating routes.ts", "Adding tests for 429 responses", "Running the test suite"],
            "done": ["Rate limiting added: 100 req/min per API key", "3 files changed · 12 tests passing"],
            "doc": {"path": "docs/RATE_LIMITS.md", "text": (
                "# Rate limits\n\nThe orders API allows **100 requests per minute per API key**.\n\n"
                "| Endpoint | Limit | Window |\n|---|---|---|\n| `POST /v2/orders` | 100 | 1 min |\n"
                "| `GET /v2/orders/:id` | 300 | 1 min |\n\nOver the limit the API answers `429 Too Many Requests`"
                " with a `Retry-After` header.\n\n## How it works\n\n- A token bucket per key, stored in Redis.\n"
                "- Buckets refill continuously; bursts up to the limit are allowed.\n- Limits are configured in "
                "`src/config/limits.ts`.\n")},
            "edits": {"src/orders/routes.ts": "\nrouter.use(rateLimit({ perMinute: 100 }));\n"},
        },
        S["dark"]: {
            "title": "Dark mode for the settings page", "prompt": "Add a dark mode toggle to the settings page",
            "model": "Sonnet 5.5", "effort": "medium", "context": 38000,
            "steps": ["Reading the theme tokens", "Adding a theme context", "Updating SettingsPage.tsx"],
            "question": "update 14 component styles to use theme tokens?",
            "after": ["Updating components", "Running the visual tests"],
            "done": ["Dark mode toggle added to Settings", "14 components use theme tokens"],
        },
        S["sync"]: {"title": "Offline sync for draft notes", "model": "Opus 5.5", "context": 92000,
                    "history": ["Offline sync for draft notes"]},
        S["hooks"]: {"title": "Retry failed webhook deliveries", "model": "Opus 5.5", "context": 54000,
                     "history": ["Retry failed webhook deliveries with backoff"]},
    },
    "projects": {
        "default": {"model": "Opus 5.5", "effort": "high", "context": 24000,
                    "steps": ["Reading the project", "Planning the change", "Writing the code", "Running the tests"],
                    "done": ["Health check endpoint added at GET /healthz", "1 file changed · tests passing"]},
    },
}

PROJECTS = {
    "atlas-api": ("feat/order-rate-limits", {
        "package.json": '{\n  "name": "atlas-api",\n  "version": "2.4.0",\n  "scripts": { "test": "vitest" }\n}\n',
        "src/orders/routes.ts": "import { Router } from 'express';\n\nexport const router = Router();\n\nrouter.post('/v2/orders', createOrder);\nrouter.get('/v2/orders/:id', getOrder);\n",
        "src/lib/redis.ts": "export const redis = createClient({ url: process.env.REDIS_URL });\n",
        "README.md": "# atlas-api\n\nOrders and invoices API.\n"}),
    "orbit-web": ("feat/dark-mode", {
        "package.json": '{\n  "name": "orbit-web",\n  "version": "1.12.0"\n}\n',
        "src/pages/SettingsPage.tsx": "export function SettingsPage() {\n  return <Page title=\"Settings\" />;\n}\n",
        "README.md": "# orbit-web\n\nCustomer dashboard.\n"}),
    "pulse-mobile": ("main", {
        "app.json": '{\n  "name": "pulse",\n  "version": "3.1.0"\n}\n',
        "src/sync/drafts.ts": "export async function syncDrafts() {}\n",
        "README.md": "# pulse-mobile\n\nNotes app.\n"}),
    "ledger-service": ("fix/webhook-retries", {
        "go.mod": "module example.com/ledger\n\ngo 1.24\n",
        "webhooks/deliver.go": "package webhooks\n\nfunc Deliver() error { return nil }\n",
        "README.md": "# ledger-service\n\nDouble-entry ledger.\n"}),
}

# (session, project, first prompt, title, last prompt, age)
PAST = [
    ("rate", "atlas-api", "Add rate limiting to the orders API (100 req/min per key)", "Add rate limiting to the orders API", None, timedelta(minutes=12)),
    ("dark", "orbit-web", "Add a dark mode toggle to the settings page", "Dark mode for the settings page", None, timedelta(minutes=34)),
    ("sync", "pulse-mobile", "Offline sync for draft notes", "Offline sync for draft notes", "Handle conflicts when two devices edit a draft", timedelta(hours=2)),
    ("hooks", "ledger-service", "Retry failed webhook deliveries with backoff", "Retry failed webhook deliveries", "Cap retries at 8 and log the final failure", timedelta(hours=3, minutes=20)),
    ("inv", "atlas-api", "Paginate the invoices endpoint", "Paginate the invoices endpoint", "Use cursor pagination, not offsets", timedelta(days=2)),
    ("price", "orbit-web", "Fix the layout shift on the pricing page", "Fix layout shift on the pricing page", None, timedelta(days=3)),
    ("cron", "ledger-service", "Move the nightly cron jobs to the queue worker", "Move cron jobs to the queue worker", None, timedelta(days=5)),
    ("rn", "pulse-mobile", "Upgrade the navigation library and fix the deep links", "Upgrade navigation and fix deep links", None, timedelta(days=8)),
]


def git(cwd, *args):
    subprocess.run(["git", "-c", "user.name=Demo", "-c", "user.email=demo@example.com", *args], cwd=cwd,
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def main():
    if os.path.isdir(D):
        shutil.rmtree(D, onerror=lambda f, p, e: (os.chmod(p, 0o777), f(p)))
    for name, (branch, files) in PROJECTS.items():
        root = os.path.join(PROJ, name)
        for rel, text in files.items():
            write(os.path.join(root, rel), text)
        git(root, "init", "-q", "-b", "main")
        git(root, "add", "-A")
        git(root, "commit", "-q", "-m", "Initial import")
        if branch != "main":
            git(root, "checkout", "-q", "-b", branch)
    # Uncommitted work so the header shows a diff.
    with open(os.path.join(PROJ, "atlas-api", "src", "orders", "routes.ts"), "a", encoding="utf-8") as f:
        f.write("\n// TODO: limit per API key\nrouter.get('/v2/orders', listOrders);\n")
    write(os.path.join(PROJ, "atlas-api", "src", "orders", "limiter.ts"),
          "export function rateLimit(opts: { perMinute: number }) {\n  // token bucket per key\n}\n")

    # Past sessions: Claude-style transcripts under env\claude\projects, file time = last activity.
    now = datetime.now(timezone.utc)
    for key, proj, first, title, last, age in PAST:
        sid = S[key]
        cwd = os.path.join(LINK, proj)
        t = now - age
        ts = t.strftime("%Y-%m-%dT%H:%M:%S.000Z")
        lines = [{"type": "user", "cwd": cwd, "sessionId": sid, "timestamp": ts,
                  "message": {"role": "user", "content": first}},
                 {"type": "assistant", "cwd": cwd, "sessionId": sid, "timestamp": ts,
                  "message": {"role": "assistant", "content": [{"type": "text", "text": "On it."}]}},
                 {"type": "ai-title", "aiTitle": title, "sessionId": sid}]
        if last:
            lines.append({"type": "last-prompt", "lastPrompt": last, "sessionId": sid})
        p = os.path.join(ENV, "claude", "projects", "D--dev-" + proj, sid + ".jsonl")
        write(p, "".join(json.dumps(l) + "\n" for l in lines))
        os.utime(p, (t.timestamp(), t.timestamp()))

    write(os.path.join(ENV, "local", "Kilno", "settings.json"), json.dumps({
        "setupDone": True, "useClaude": True, "useCodex": False, "updateCheck": False,
        "screenshotHotkey": False, "screenshotOffer": True, "theme": "numbers", "reserveAt": 0}, indent=2))
    os.makedirs(os.path.join(ENV, "codex"), exist_ok=True)
    os.makedirs(os.path.join(ENV, "tmp"), exist_ok=True)

    # Stand-in agent, its scenarios and a short command on PATH (what Kilno types into the pane).
    agent_dir = os.path.join(D, "agent")
    os.makedirs(os.path.join(agent_dir, "ctl"), exist_ok=True)
    shutil.copyfile(os.path.join(HERE, "demo_agent.py"), os.path.join(agent_dir, "demo_agent.py"))
    write(os.path.join(agent_dir, "scenarios.json"), json.dumps(SCEN, indent=1))
    py = os.path.join(REPO, ".launchkit", "venv", "Scripts", "python.exe")
    write(os.path.join(D, "bin", "demo-agent.cmd"),
          f'@echo off\r\n"{py}" -X utf8 "{os.path.join(agent_dir, "demo_agent.py")}" %*\r\n')

    # The "other app": a docs page shown in an isolated Edge app window.
    write(os.path.join(D, "site", "index.html"), DOCS_PAGE)
    write(os.path.join(D, "site", "backdrop.html"),
          "<!doctype html><title>Desk</title><body style='margin:0;background:#101317'></body>")

    # Junction D:\dev -> demo-data\projects (no admin needed), so the app shows short paths.
    if os.path.lexists(LINK):
        subprocess.run(["cmd", "/c", "rmdir", LINK], check=False)
    subprocess.run(["cmd", "/c", "mklink", "/J", LINK, PROJ], check=True, stdout=subprocess.DEVNULL)
    json.dump(S, open(os.path.join(D, "sessions.json"), "w"), indent=1)
    print("demo-data ready:", D)


DOCS_PAGE = """<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Rate limiting · Atlas docs</title>
<style>
body{margin:0;font:17px/1.65 'Segoe UI',sans-serif;background:#f7f7f5;color:#22262b}
header{background:#1d2229;color:#e8eaed;padding:18px 48px;font-weight:600;letter-spacing:.3px}
header span{color:#d4a24c}
main{max-width:860px;margin:40px auto;padding:0 32px}
h1{font-size:38px;margin:.2em 0 .4em}h2{margin-top:1.8em}
code{background:#eceae4;padding:2px 6px;border-radius:4px;font:15px Cascadia Mono,Consolas,monospace}
pre{background:#1d2229;color:#dfe3e8;padding:18px 22px;border-radius:10px;font:15px/1.5 Cascadia Mono,Consolas,monospace}
.note{border-left:4px solid #d4a24c;background:#fff;padding:12px 18px;border-radius:6px}
</style></head><body><header><span>◆</span> Atlas · API guide</header><main>
<h1>Rate limiting</h1>
<p>Every API key gets a budget of requests per minute. Bursts are fine; sustained traffic above the
budget gets <code>429 Too Many Requests</code> with a <code>Retry-After</code> header.</p>
<div class="note">Plan your batch jobs around the limit: spread writes, back off on 429.</div>
<h2>Token buckets</h2>
<p>Each key owns a bucket that refills continuously. A request takes one token; an empty bucket
means wait.</p>
<pre>limiter.take(key, 1)
  .then(next)
  .catch(() =&gt; res.status(429).set('Retry-After', '12'))</pre>
<h2>Choosing limits</h2>
<p>Start from your p99 traffic per customer, add headroom for retries, and review monthly.</p>
</main></body></html>
"""

if __name__ == "__main__":
    main()
