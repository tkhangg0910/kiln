"""Phase 9: demo.gif (< 8 MB), demo.webm and poster.png from promo.mp4; lossless PNG optimisation
of docs/assets. Usage: python readme_media.py <start_s> <end_s> <poster_s>"""
import os, subprocess, sys
from PIL import Image
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
A = os.path.join(REPO, "docs", "assets")
promo = os.path.join(REPO, "docs", "demo", "promo.mp4")
ss, to, poster = sys.argv[1], sys.argv[2], sys.argv[3]
dur = f"{float(to) - float(ss):.2f}"
gif = os.path.join(A, "demo.gif")
for width, fps in [(960, 15), (960, 12), (800, 12), (720, 10)]:
    vf = f"fps={fps},scale={width}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle"
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", ss, "-t", dur, "-i", promo, "-vf", vf, "-loop", "0", gif], check=True)
    mb = os.path.getsize(gif) / 1048576
    print(f"demo.gif {width}px {fps} fps: {mb:.2f} MB")
    if mb < 8:
        break
subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", ss, "-t", dur, "-i", promo, "-an", "-c:v", "libvpx-vp9", "-b:v", "0",
                "-crf", "30", "-row-mt", "1", "-deadline", "good", "-cpu-used", "2", os.path.join(A, "demo.webm")], check=True)
print(f"demo.webm: {os.path.getsize(os.path.join(A, 'demo.webm')) / 1048576:.2f} MB")
subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", poster, "-i", promo, "-frames:v", "1", os.path.join(A, "poster.png")], check=True)
for f in sorted(os.listdir(A)):
    if f.endswith(".png"):
        p = os.path.join(A, f)
        before = os.path.getsize(p)
        im = Image.open(p)
        im.save(p, optimize=True)
        print(f"{f}: {im.size[0]}x{im.size[1]}, {before // 1024} -> {os.path.getsize(p) // 1024} KB")
