#!/usr/bin/env python3
"""Regenerate the compact ARIB map from the pinned TSDuck checkout."""

import argparse
import re
import struct
import subprocess
from pathlib import Path

REVISION = "f3e026e927c5b054649ff4e5d5a6ba2eabc1ce8c"
SOURCE = "src/libtsduck/dtv/charset/tsARIBCharsetEncoding.cpp"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tsduck", type=Path)
    args = parser.parse_args()
    source = subprocess.check_output(
        ["git", "-C", str(args.tsduck), "show", f"{REVISION}:{SOURCE}"], text=True
    )
    entries = [
        (int(code, 16), int(entry, 16))
        for code, entry in re.findall(r"\{0x([0-9A-Fa-f]+), 0x([0-9A-Fa-f]+)\}", source)
    ]
    if len(entries) != 6647 or any(
        count == 0 or start + count > following
        for (start, entry), (following, _) in zip(entries, entries[1:])
        for count in [entry & 255]
    ):
        raise ValueError("Unexpected TSDuck encoding table")
    output = Path(__file__).resolve().parents[1] / "src/arib_text/encoding.bin"
    output.parent.mkdir(exist_ok=True)
    output.write_bytes(b"".join(struct.pack(">II", *entry) for entry in entries))
    print(f"Wrote {len(entries)} ranges ({output.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
