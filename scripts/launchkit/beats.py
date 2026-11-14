"""Beat grid of the BGM: spectral-flux onsets, then the BPM (near a hint) and phase whose grid
lands on the most onset energy. Usage: python beats.py <audio> [bpm_hint]  -> prints JSON."""
import json, subprocess, sys
import numpy as np

path, hint = sys.argv[1], float(sys.argv[2]) if len(sys.argv) > 2 else 120
sr, hop, n = 22050, 256, 2048
raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(sr), "-f", "f32le", "-"],
                     capture_output=True, check=True).stdout
x = np.frombuffer(raw, dtype=np.float32)
win = np.hanning(n)
frames = np.lib.stride_tricks.sliding_window_view(x, n)[::hop] * win
mag = np.abs(np.fft.rfft(frames, axis=1))
flux = np.maximum(0, np.diff(np.log1p(mag), axis=0)).sum(axis=1)
flux = (flux - flux.mean()) / (flux.std() + 1e-9)
fps = sr / hop
best = None
for bpm in np.arange(hint - 3, hint + 3, 0.02):
    period = 60 / bpm * fps
    for phase in np.arange(0, period, 0.5):
        idx = (phase + period * np.arange(int((len(flux) - phase) / period))).astype(int)
        score = flux[idx].mean()
        if best is None or score > best[0]:
            best = (score, bpm, phase / fps)
score, bpm, offset = best
# Coverage check: share of strong onsets within 50 ms of a grid line.
peaks = np.where((flux[1:-1] > flux[:-2]) & (flux[1:-1] > flux[2:]) & (flux[1:-1] > 1.5))[0] + 1
t = peaks / fps
beat = 60 / bpm
dist = np.abs(((t - offset + beat / 2) % beat) - beat / 2)
print(json.dumps({"bpm": round(float(bpm), 3), "offset_s": round(float(offset), 4),
                  "beat_s": round(beat, 5), "duration_s": round(len(x) / sr, 2),
                  "coverage_50ms": round(float((dist < 0.05).mean()), 3), "score": round(float(score), 3)}))
