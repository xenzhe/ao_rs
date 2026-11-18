"""Unpacks the resource packs of the official Argentum 20 client (Steam install).

Format (argentum-online-client-classic, modCompression.bas):
  FILEHEADER  = Long size, Integer count, String*32 md5(password)        (38 bytes)
  INFOHEADER  = Long start (1-based), Long size, String*32 name, Long raw (44 bytes)
  data        = XOR with the password (DoCrypt_Data), then AES-128-CFB with a fixed
                key/iv (Decompress_Data), then zlib.
The password comes from Recursos/OUTPUT/AO.bin (General.bas CheckResources).

Usage: python -I unpack_official.py "<Argentum 20>/Recursos" <out dir> [pack ...]
Packs: Graficos init Mapas MiniMapas Interface Sounds OGG Midi (default: all).
"""
import os
import struct
import subprocess
import sys
import zlib
from concurrent.futures import ThreadPoolExecutor

KEY = "AB45456789ABCDEFA0E1D2C3B4A59666"
IV = "FEDCAA9A76543A10FEDCBA9876543322"
OPENSSL = os.environ.get("OPENSSL", "openssl")


def password(recursos):
    data = open(os.path.join(recursos, "OUTPUT", "AO.bin"), "rb").read()
    length = data[199] + data[198] * 256
    # VB data(1 To 200): data(i * 3 - 1) is Python index i * 3 - 2.
    return bytes(data[i * 3 - 2] ^ 37 for i in range(1, length + 1))


def xor(buf, pw):
    n = len(pw)
    out = bytearray(buf)
    c = (len(buf) - 1) % n  # 0-based of VB's `UBound Mod Len + 1`
    for i in range(len(out)):
        out[i] ^= pw[c]
        c = c - 1 if c > 0 else n - 1
    return bytes(out)


def aes_cfb(buf):
    r = subprocess.run([OPENSSL, "enc", "-d", "-aes-128-cfb", "-K", KEY, "-iv", IV, "-nopad"], input=buf, capture_output=True)
    if r.returncode != 0:
        raise RuntimeError(r.stderr.decode(errors="replace"))
    return r.stdout


def entries(path):
    f = open(path, "rb")
    size, count = struct.unpack("<ih", f.read(6))
    f.read(32)
    out = []
    for _ in range(count):
        start, csize = struct.unpack("<ii", f.read(8))
        name = f.read(32).decode("latin1").rstrip(" \0")
        (raw,) = struct.unpack("<i", f.read(4))
        out.append((name, start - 1, csize, raw))
    return out


def unpack(pack, out_dir, pw):
    os.makedirs(out_dir, exist_ok=True)
    blob = open(pack, "rb").read()
    items = entries(pack)

    def one(e):
        name, start, csize, raw = e
        data = xor(blob[start:start + csize], pw)
        data = zlib.decompress(aes_cfb(data))
        open(os.path.join(out_dir, name), "wb").write(data)
        return len(data) == raw

    with ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as ex:
        ok = sum(ex.map(one, items))
    return len(items), ok


def main():
    recursos, out = sys.argv[1], sys.argv[2]
    packs = sys.argv[3:] or ["Graficos", "init", "Mapas", "MiniMapas", "Interface", "Sounds", "OGG", "Midi"]
    pw = password(recursos)
    for p in packs:
        path = os.path.join(recursos, "OUTPUT", p)
        if not os.path.isfile(path):
            print(f"{p}: no existe")
            continue
        n, ok = unpack(path, os.path.join(out, p), pw)
        print(f"{p}: {n} archivos, {ok} con el tamano correcto")


if __name__ == "__main__":
    main()
