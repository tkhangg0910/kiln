"""Contact sheet of review frames: python contact.py <out.png> <frame dir or files...>"""
import os, sys
from PIL import Image, ImageDraw
out, srcs = sys.argv[1], sys.argv[2:]
files = []
for s in srcs:
    files += [os.path.join(s, f) for f in sorted(os.listdir(s))] if os.path.isdir(s) else [s]
ims = [Image.open(f).convert("RGB") for f in files]
w = 640
th = [int(i.height * w / i.width) for i in ims]
cols = 2
rows = (len(ims) + cols - 1) // cols
H = max(th) + 26
sheet = Image.new("RGB", (cols * w + (cols + 1) * 8, rows * H + 8), (40, 40, 40))
d = ImageDraw.Draw(sheet)
for k, (im, f) in enumerate(zip(ims, files)):
    x, y = 8 + (k % cols) * (w + 8), 8 + (k // cols) * H
    sheet.paste(im.resize((w, th[k])), (x, y + 20))
    d.text((x, y + 4), os.path.relpath(f, os.path.dirname(os.path.dirname(f))), fill=(255, 220, 120))
sheet.save(out)
print(out, sheet.size)
