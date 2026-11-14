"""Phase 6 check of the raw clips: ffprobe (size, fps, duration, frame count) and frames for review
(first, last, and one at every click/key event) into docs/demo/frames/<scene>/.
Usage: python check_clips.py [scene ...]"""
import json, os, subprocess, sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
RAW = os.path.join(REPO, "docs", "demo", "raw")
FR = os.path.join(REPO, "docs", "demo", "frames")


def probe(mp4):
    out = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-count_frames", "-show_entries",
                          "stream=width,height,r_frame_rate,nb_read_frames:format=duration", "-of", "json", mp4],
                         capture_output=True, text=True).stdout
    j = json.loads(out)
    st, fmt = j["streams"][0], j["format"]
    n, d = map(int, st["r_frame_rate"].split("/"))
    return st["width"], st["height"], n / d, float(fmt["duration"]), int(st["nb_read_frames"])


def frame(mp4, t, path):
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{max(0, t):.3f}", "-i", mp4, "-frames:v", "1",
                    "-vf", "scale=960:-1", path], check=True)


def main():
    scenes = sys.argv[1:] or sorted(f[:-4] for f in os.listdir(RAW) if f.endswith(".mp4"))
    for sc in scenes:
        mp4 = os.path.join(RAW, sc + ".mp4")
        ev = json.load(open(os.path.join(RAW, sc + ".events.json"), encoding="utf-8"))
        w, h, fps, dur, n = probe(mp4)
        real_fps = n / dur if dur else 0
        print(f"{sc}: {w}x{h} {fps:.0f} fps nominal, {n} frames / {dur:.2f} s = {real_fps:.1f} fps; events {len(ev['events'])}, "
              f"recorded {ev['duration_ms'] / 1000:.2f} s")
        d = os.path.join(FR, sc)
        os.makedirs(d, exist_ok=True)
        for f in os.listdir(d):
            os.remove(os.path.join(d, f))
        frame(mp4, 0.05, os.path.join(d, "00-first.png"))
        frame(mp4, dur - 0.1, os.path.join(d, "99-last.png"))
        k = 0
        for e in ev["events"]:
            if e["type"] in ("click", "key"):
                k += 1
                # Just after the action, when its effect starts to show.
                frame(mp4, e["t_ms"] / 1000 + 0.25, os.path.join(d, f"{k:02d}-{e['type']}-{e['t_ms']}ms.png"))


if __name__ == "__main__":
    main()
