"""1 frame per second of a video into docs/demo/frames/review/<name>/ and 20-frame sheets.
Usage: python review_sheets.py <video.mp4> <name>"""
import os, subprocess, sys
from PIL import Image, ImageDraw
video, name = sys.argv[1], sys.argv[2]
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
d = os.path.join(REPO, "docs", "demo", "frames", "review", name)
os.makedirs(d, exist_ok=True)
for f in os.listdir(d):
    os.remove(os.path.join(d, f))
subprocess.run(["ffmpeg", "-v", "error", "-i", video, "-vf", "fps=1,scale=640:-1", os.path.join(d, "%03d.png")], check=True)
fs = sorted(os.listdir(d))
for part in range(0, len(fs), 20):
    ims = [Image.open(os.path.join(d, f)) for f in fs[part:part + 20]]
    w, h = ims[0].size
    sh = Image.new("RGB", (4 * w, ((len(ims) + 3) // 4) * (h + 18)), (30, 30, 30))
    dr = ImageDraw.Draw(sh)
    for i, im in enumerate(ims):
        x, y = (i % 4) * w, (i // 4) * (h + 18)
        sh.paste(im, (x, y + 18))
        dr.text((x + 4, y + 3), f"t={part + i}s", fill=(255, 220, 120))
    out = os.path.join(REPO, ".launchkit", f"review-{name}-{part // 20}.png")
    sh.save(out)
    print(out)
