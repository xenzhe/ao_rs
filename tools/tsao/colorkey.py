"""TSAO graphics are opaque RGB with pure black as the transparent colour (DX8 colour
key). Rewrites every PNG of a folder as RGBA with black -> alpha 0.

usage: python -I colorkey.py <Graficos dir>
"""
import os
import sys

from PIL import Image


def main(folder):
    n = 0
    for f in os.listdir(folder):
        if not f.lower().endswith(".png"):
            continue
        p = os.path.join(folder, f)
        im = Image.open(p)
        if im.mode == "RGBA" and im.getextrema()[3][0] < 255:
            continue  # already has transparency
        im = im.convert("RGB")
        a = im.convert("L").point(lambda v: 255)
        px = im.load()
        ap = a.load()
        w, h = im.size
        for y in range(h):
            for x in range(w):
                if px[x, y] == (0, 0, 0):
                    ap[x, y] = 0
        im.putalpha(a)
        im.save(p, optimize=False)
        n += 1
    print(f"{n} images keyed")


if __name__ == "__main__":
    main(sys.argv[1])
