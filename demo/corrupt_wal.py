#!/usr/bin/env python3
"""Flip one byte inside the first WAL entry's value so its CRC32 no longer matches.

Used by the demo to show that corruption is detected rather than served, and that the
reader can scan past a bad entry to the next magic marker instead of giving up.
"""
import sys

path = "data/wal"
data = bytearray(open(path, "rb").read())
needle = sys.argv[1].encode() if len(sys.argv) > 1 else b"seattle"

i = data.find(needle)
if i == -1:
    sys.exit(f"{needle!r} not found in {path}")

data[i + 2] ^= 0xFF
open(path, "wb").write(data)
print(f"flipped one byte at offset {i + 2}, inside the value {needle.decode()!r}")
