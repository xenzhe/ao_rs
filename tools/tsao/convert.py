"""Converts the Tierras Sagradas (AO 0.11.5 DX8) client/server data into the AO20-style
`Recursos` layout the engine reads: init/*.ini indexes, Mapas/mapaN.csm, Dat/.

usage: python -I convert.py <tsao_root> <out_root>
Graphics come from unpack_grh.py (out_root/Graficos).
"""
import os
import re
import struct
import sys

HEADER = 263  # tCabecera: Desc*255 + CRC + MagicWord


def read_ind_graphics(path):
    """Graficos.ind -> {grh: (file, sx, sy, w, h)} | {grh: ([frames], speed)}."""
    d = open(path, "rb").read()
    _ver, count = struct.unpack_from("<ii", d, 0)
    p, out = 8, {}
    while p + 6 <= len(d):
        grh, nf = struct.unpack_from("<ih", d, p)
        p += 6
        if grh <= 0 or nf <= 0:
            break
        if nf > 1:
            frames = struct.unpack_from("<%di" % nf, d, p)
            p += 4 * nf
            (speed,) = struct.unpack_from("<f", d, p)
            p += 4
            out[grh] = (list(frames), speed)
        else:
            out[grh] = struct.unpack_from("<ihhhh", d, p)
            p += 12
    return out


def write_graficos(grhs, path):
    lines = ["[INIT]", f"NumGrh={max(grhs)}", "", "[Graphics]"]
    for g in sorted(grhs):
        v = grhs[g]
        if isinstance(v[0], list):
            frames, speed = v
            lines.append(f"Grh{g}={len(frames)}-" + "-".join(map(str, frames)) + f"-{speed:g}")
        else:
            lines.append(f"Grh{g}=1-" + "-".join(map(str, v)))
    open(path, "w", newline="\r\n").write("\n".join(lines) + "\n")


def read_ind_records(path, fmt):
    d = open(path, "rb").read()
    (n,) = struct.unpack_from("<h", d, HEADER)
    size = struct.calcsize(fmt)
    return [struct.unpack_from(fmt, d, HEADER + 2 + i * size) for i in range(n)]


def write_sections(path, count_key, n, prefix, rows):
    lines = ["[INIT]", f"{count_key}={n}", ""]
    for i, kv in rows:
        lines.append(f"[{prefix}{i}]")
        lines += [f"{k}={v}" for k, v in kv]
        lines.append("")
    open(path, "w", newline="\r\n").write("\n".join(lines))


def clean_ini(src, dst):
    """Copies an INI dropping the ' comments after values."""
    text = open(src, encoding="latin-1").read()
    text = re.sub(r"[ \t]*'[^\r\n]*", "", text)
    open(dst, "w", encoding="latin-1", newline="\r\n").write(text.replace("\r\n", "\n"))


def ini_sections(path):
    out, cur = {}, None
    for line in open(path, encoding="latin-1"):
        line = line.split("'")[0].strip()
        if line.startswith("[") and line.endswith("]"):
            cur = out.setdefault(line[1:-1].lower(), {})
        elif "=" in line and cur is not None:
            k, v = line.split("=", 1)
            cur[k.strip().lower()] = v.strip()
    return out


def vb_string(s):
    b = s.encode("cp1252", "replace")
    return struct.pack("<h", len(b)) + b


def convert_map(num, maps_dir, info):
    """0.11.5 .map/.inf -> legacy 4-layer .csm (file layers 1..4 -> engine 1,2,4,5)."""
    m = open(os.path.join(maps_dir, f"Mapa{num}.map"), "rb").read()
    inf_path = os.path.join(maps_dir, f"Mapa{num}.inf")
    inf = open(inf_path, "rb").read()
    pm, pi = 273, 10
    blocked, layers, triggers, particles, lights, objs, npcs, exits = [], [[], [], [], []], [], [], [], [], [], []
    for y in range(1, 101):
        for x in range(1, 101):
            f = m[pm]
            pm += 1
            if f & 1:
                blocked.append((x, y, 0x0F))
            for li, bit in enumerate((0, 2, 4, 8)):
                if li == 0 or f & bit:
                    (g,) = struct.unpack_from("<H", m, pm)
                    pm += 2
                    if g:
                        layers[li].append((x, y, g))
            if f & 16:
                (t,) = struct.unpack_from("<h", m, pm)
                pm += 2
                if t:
                    triggers.append((x, y, t))
            if f & 32:
                (pg,) = struct.unpack_from("<h", m, pm)
                pm += 2
                if pg > 0:
                    particles.append((x, y, pg))
            if f & 64:
                rng, r, g, b = struct.unpack_from("<4h", m, pm)
                pm += 8
                if rng > 0:
                    lights.append((x, y, r & 255, g & 255, b & 255, min(rng, 255)))
            fi = inf[pi]
            pi += 1
            if fi & 1:
                em, ex, ey = struct.unpack_from("<3h", inf, pi)
                pi += 6
                if em > 0:
                    exits.append((x, y, em, ex, ey))
            if fi & 2:
                (npc,) = struct.unpack_from("<h", inf, pi)
                pi += 2
                if npc > 0:
                    npcs.append((x, y, npc))
            if fi & 4:
                oi, amount = struct.unpack_from("<2h", inf, pi)
                pi += 4
                if oi > 0:
                    objs.append((x, y, oi, amount))
    out = bytearray()
    out += struct.pack("<i", len(blocked))
    out += struct.pack("<4i", *(len(l) for l in layers))
    out += struct.pack("<6i", len(triggers), len(lights), len(particles), len(npcs), len(objs), len(exits))
    out += struct.pack("<ii", 0, 0)  # tMapSize
    out += vb_string(info.get("name", f"Mapa {num}"))
    out += b"\x00" + vb_string("") + struct.pack("<ii", int(info.get("musicnum", "0") or 0), 0)
    out += bytes([1 if info.get("pk", "1").strip() == "0" else 0])
    out += vb_string(info.get("zona", "")) + vb_string(info.get("terreno", "")) + vb_string("")
    out += bytes(16) + vb_string("")
    out += bytes([0, 0, 0])  # rain, snow, fog
    for x, y, v in blocked:
        out += struct.pack("<hhB", x, y, v)
    for l in layers:
        for x, y, g in l:
            out += struct.pack("<hhi", x, y, g)
    for x, y, t in triggers:
        out += struct.pack("<hhh", x, y, t)
    for x, y, pg in particles:
        out += struct.pack("<hhi", x, y, pg)
    for x, y, r, g, b, rng in lights:
        out += struct.pack("<hhBBBBB", x, y, b, g, r, 255, rng)
    for x, y, oi, amount in objs:
        out += struct.pack("<hhhh", x, y, oi, amount)
    for x, y, npc in npcs:
        out += struct.pack("<hhh", x, y, npc)
    for x, y, em, ex, ey in exits:
        out += struct.pack("<hhhhh", x, y, em, ex, ey)
    return bytes(out), info.get("name", "")


AO20_DAT = r"X:/dev/ao20/Recursos/Dat"


def convert_dats(src, dst):
    """TSAO world data (NPCs, objects, spells) over the AO20 files the engine also needs
    (balance, crafting, quests...) until each system is ported."""
    import shutil
    sdat = os.path.join(src, "Servidor", "Dat")
    ddat = os.path.join(dst, "Dat")
    if os.path.isdir(ddat):
        shutil.rmtree(ddat)
    shutil.copytree(AO20_DAT, ddat)
    def read(f):
        b = open(os.path.join(sdat, f), "rb").read()
        if b[:2] == b"\xff\xfe":  # some TSAO files were saved as UTF-16
            return b[2:].decode("utf-16-le")
        return b.decode("latin-1")
    # NPCs: friendly (< 500) and hostile (>= 500) live in two files in 0.11.5.
    npcs = read("NPCs.dat") + "\n" + re.sub(r"(?im)^\[INIT\][^\[]*", "", read("NPCs-HOSTILES.dat"))
    for f in os.listdir(ddat):
        if f.lower() in ("npcs.dat", "obj.dat", "hechizos.dat"):
            os.remove(os.path.join(ddat, f))
    open(os.path.join(ddat, "npcs.dat"), "w", encoding="latin-1", newline="\r\n").write(npcs.replace("\r\n", "\n"))
    for f, out in (("Obj.dat", "obj.dat"), ("Hechizos.dat", "Hechizos.dat")):
        open(os.path.join(ddat, out), "w", encoding="latin-1", newline="\r\n").write(read(f).replace("\r\n", "\n"))


def main(src, dst):
    cinit = os.path.join(src, "Cliente", "Data", "INIT")
    init = os.path.join(dst, "init")
    os.makedirs(init, exist_ok=True)
    grhs = read_ind_graphics(os.path.join(cinit, "Graficos.ind"))
    write_graficos(grhs, os.path.join(init, "graficos.ini"))
    bodies = read_ind_records(os.path.join(cinit, "Personajes.ind"), "<6h")
    write_sections(os.path.join(init, "cuerpos.dat"), "NumBodies", len(bodies), "BODY",
                   [(i + 1, [(f"Walk{k + 1}", b[k] & 0xFFFF) for k in range(4)] + [("HeadOffsetX", b[4]), ("HeadOffsetY", b[5])]) for i, b in enumerate(bodies) if b[0]])
    for name, out, key in (("Cabezas.ind", "cabezas.ini", "NumHeads"), ("Cascos.ind", "cascos.ini", "NumHeads")):
        heads = read_ind_records(os.path.join(cinit, name), "<4h")
        write_sections(os.path.join(init, out), key, len(heads), "HEAD",
                       [(i + 1, [(f"Head{k + 1}", h[k] & 0xFFFF) for k in range(4)]) for i, h in enumerate(heads) if h[0]])
    fxs = read_ind_records(os.path.join(cinit, "Fxs.ind"), "<3h")
    write_sections(os.path.join(init, "FXs.ini"), "NumFxs", len(fxs), "FX",
                   [(i + 1, [("Animacion", f[0] & 0xFFFF), ("OffsetX", f[1]), ("OffsetY", f[2]), ("IsPNG", 1)]) for i, f in enumerate(fxs)])
    clean_ini(os.path.join(cinit, "Armas.dat"), os.path.join(init, "armas.dat"))
    clean_ini(os.path.join(cinit, "Escudos.dat"), os.path.join(init, "escudos.dat"))
    for empty in ("moldes.ini", "TreeGrh.dat"):
        open(os.path.join(init, empty), "w").write("")
    # Maps.
    smaps = os.path.join(src, "Servidor", "Maps")
    mdir = os.path.join(dst, "Mapas")
    os.makedirs(mdir, exist_ok=True)
    names, ok, bad = [], 0, []
    nums = sorted({int(m.group(1)) for f in os.listdir(smaps) if (m := re.match(r"(?i)mapa(\d+)\.map$", f))})
    for n in nums:
        dat = os.path.join(smaps, f"Mapa{n}.dat")
        info = ini_sections(dat).get(f"mapa{n}", {}) if os.path.exists(dat) else {}
        try:
            data, name = convert_map(n, smaps, info)
            open(os.path.join(mdir, f"mapa{n}.csm"), "wb").write(data)
            names.append(f"mapa{n}={name}")
            ok += 1
        except Exception as e:
            bad.append((n, repr(e)))
    open(os.path.join(init, "namemapa.dat"), "w", encoding="cp1252", newline="\r\n").write("[NameMapa]\n" + "\n".join(names) + "\n")
    convert_dats(src, dst)
    print(f"grh {len(grhs)}, bodies {len(bodies)}, maps ok {ok}, failed {bad[:5]}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
