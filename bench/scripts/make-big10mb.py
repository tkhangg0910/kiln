"""Writes big10mb.txt (10 MiB of ASCII, Vietnamese and SGR colour) into the bench folder."""
import os, random, sys
d = os.environ.get("KILN_BENCH_DIR") or os.path.join(os.environ["TEMP"], "kiln-bench")
os.makedirs(d, exist_ok=True)
random.seed(1)
with open(os.path.join(d, "big10mb.txt"), "w", encoding="utf-8", newline="\n") as f:
    n = 0
    while n < 10 * 1024 * 1024:
        line = f"{n:010d} " + "".join(random.choice("abcdefghij klmnopqrstuvwxyz ÄĐếữ\x1b[32mok\x1b[0m") for _ in range(100)) + "\n"
        f.write(line)
        n += len(line.encode())
