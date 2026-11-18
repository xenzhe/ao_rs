"""Extracts a Tierras Sagradas resource pack (Grh.tsao) into a folder.

Format (client modCompression.bas): FILEHEADER {i32 size + 435098, i16 count}, then
`count` INFOHEADER {i32 start (1-based), i32 packed size, char[32] name, i32 raw size};
each chunk is zlib data whose first len(key) bytes are XORed with the key "relokard0".

usage: python -I unpack_grh.py <pack> <out_dir>
"""
import os
import struct
import sys
import zlib

KEY = b"relokard0"


def main(pack, out):
    data = open(pack, "rb").read()
    _size, count = struct.unpack_from("<ih", data, 0)
    os.makedirs(out, exist_ok=True)
    pos, ok, bad = 6, 0, []
    for _ in range(count):
        start, packed, name, _raw = struct.unpack_from("<ii32si", data, pos)
        pos += 44
        name = name.decode("latin-1").strip().strip("\0")
        chunk = bytearray(data[start - 1:start - 1 + packed])
        for i, k in enumerate(KEY[:len(chunk)]):
            chunk[i] ^= k
        try:
            body = zlib.decompressobj().decompress(bytes(chunk))
            if body[:4] != b"\x89PNG" and body[:2] != b"BM":
                raise ValueError(body[:8])
            open(os.path.join(out, name), "wb").write(body)
            ok += 1
        except Exception as e:  # keep going, report at the end
            bad.append((name, repr(e)))
    print(f"{ok} extracted, {len(bad)} failed", bad[:5])


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
