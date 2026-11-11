"""Crops each screenshot below its last non-background row (plus a margin) and stores it as a
palette PNG: the screenshots of results/screenshots/ at a size fit for the repository."""
import sys
from pathlib import Path
from PIL import Image

BG = (0xF5, 0xF6, 0xF8)
for path in sorted(Path(sys.argv[1]).glob("*.png")):
    im = Image.open(path).convert("RGB")
    w, h = im.size
    px = im.load()
    last = h - 1
    while last > 0 and all(px[x, last] == BG for x in range(0, w, 4)):
        last -= 1
    im = im.crop((0, 0, w, min(h, last + 24)))
    im.quantize(colors=128, method=Image.Quantize.MEDIANCUT).save(path, optimize=True)
    print(path.name, im.size[1], path.stat().st_size)
