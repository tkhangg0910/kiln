"""Phase 8 post-production: two-pass loudness normalisation to -14 LUFS (true peak -1.5 dB) for
both videos, and a logo watermark bottom-right on the full demo. Inputs are never modified.
Usage: python finish_videos.py <promo-raw.mp4> <full-raw.mp4> <logo.png> <out-dir>"""
import json, os, re, subprocess, sys

promo, full, logo, out = sys.argv[1:5]
TARGET = "I=-14:TP=-1.5:LRA=11"


def measure(path):
    r = subprocess.run(["ffmpeg", "-hide_banner", "-i", path, "-af", f"loudnorm={TARGET}:print_format=json",
                        "-f", "null", "-"], capture_output=True, text=True)
    return json.loads(re.findall(r"\{[^{}]+\}", r.stderr)[-1])


def finish(src, dst, watermark):
    m = measure(src)
    # Fixed gain to -14 LUFS, then a look-ahead limiter at -3 dBFS so the true peak stays under
    # -1.5 dB after AAC (linear loudnorm cannot both raise the level and hold the peak).
    gain = -14.0 - float(m["input_i"])
    af = f"volume={gain:.2f}dB,alimiter=limit=0.70:attack=5:release=60:level=false,aresample=48000"
    cmd = ["ffmpeg", "-hide_banner", "-v", "error", "-y", "-i", src]
    if watermark:
        # 64 px logo at 60 % opacity, 36 px from the bottom-right corner, over the gradient margin.
        cmd += ["-i", logo, "-filter_complex",
                "[1]scale=64:64:flags=neighbor,format=rgba,colorchannelmixer=aa=0.6[l];[0][l]overlay=W-w-36:H-h-36[v]",
                "-map", "[v]", "-map", "0:a", "-c:v", "libx264", "-preset", "slow", "-crf", "17",
                "-pix_fmt", "yuv420p", "-r", "60"]
    else:
        cmd += ["-map", "0:v", "-map", "0:a", "-c:v", "copy"]
    cmd += ["-af", af, "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", dst]
    subprocess.run(cmd, check=True)
    after = measure(dst)
    print(f"{os.path.basename(dst)}: {m['input_i']} LUFS -> {after['input_i']} LUFS, true peak {after['input_tp']} dB")


finish(promo, os.path.join(out, "promo.mp4"), watermark=False)
finish(full, os.path.join(out, "demo-full.mp4"), watermark=True)
