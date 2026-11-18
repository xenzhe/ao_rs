"""TSAO's DX8 client sampled textures with wrap addressing, so some GRHs point past the
right/bottom edge of their image (water frames at x=256 of a 256-wide sheet) and were
drawn from the start again. The engine clamps; rewrite those source rectangles modulo the
image size. Reports how many GRHs were fixed and which still overflow.

usage: python -I wrapfix.py <out_root>
"""
import os
import re
import sys

from PIL import Image


def main(root):
    ini = os.path.join(root, "init", "graficos.ini")
    text = open(ini, encoding="latin-1").read()
    sizes = {}
    fixed, bad = 0, []

    def size(f):
        if f not in sizes:
            p = os.path.join(root, "Graficos", f"{f}.png")
            sizes[f] = Image.open(p).size if os.path.exists(p) else None
        return sizes[f]

    def fix(m):
        nonlocal fixed
        g, f, sx, sy, w, h = (int(v) for v in m.groups())
        s = size(f)
        if not s:
            return m.group(0)
        iw, ih = s
        nx, ny = sx, sy
        if sx + w > iw and iw > 0:
            nx = sx % iw
        if sy + h > ih and ih > 0:
            ny = sy % ih
        # A frame a few pixels bigger than its image (sprite sheets cut short): clamp it.
        nw, nh = min(w, iw - nx), min(h, ih - ny)
        if (nx, ny, nw, nh) != (sx, sy, w, h):
            fixed += 1
            if nw <= 0 or nh <= 0:
                bad.append(g)
            return f"Grh{g}=1-{f}-{nx}-{ny}-{max(nw, 1)}-{max(nh, 1)}"
        return m.group(0)

    text = re.sub(r"(?m)^Grh(\d+)=1-(\d+)-(\d+)-(\d+)-(\d+)-(\d+)\s*$", fix, text)
    open(ini, "w", encoding="latin-1", newline="\r\n").write(text.replace("\r\n", "\n"))
    print(f"{fixed} GRHs wrapped, {len(bad)} still overflow", bad[:10])


if __name__ == "__main__":
    main(sys.argv[1])
