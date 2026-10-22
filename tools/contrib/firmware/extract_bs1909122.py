#!/usr/bin/env python3
import argparse
import hashlib
import json
import struct
import zlib
from pathlib import Path

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes


DECODER_SHA256 = "23ce8dad0b5bbd4217fc876ce94a213c3c6daa982aaf651d4ef90562fd77cd1e"
UPDATE_SHA256 = "a8210d4aecf4c667cd7bfd0263eef7b351115cf76e07c8ce0c574f4409568ca8"
ENCODINGS = {1: "AES-128-CBC", 2: "raw", 4: "raw", 8: "AES-128-CBC", 9: "raw", 10: "AES-128-CBC"}


def read_verified(path: Path, expected: str) -> bytes:
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError(f"未照合の入力ファイルです: {path}")
    return data


def decrypt(data: bytes, key: bytes, iv: bytes) -> bytes:
    decoder = Cipher(algorithms.AES(key), modes.CBC(iv)).decryptor()
    return decoder.update(data) + decoder.finalize()


def write_new(path: Path, data: bytes) -> None:
    with path.open("xb") as output:
        output.write(data)


def extract(decoder_path: Path, update_path: Path, output: Path) -> None:
    decoder = read_verified(decoder_path, DECODER_SHA256)
    update = read_verified(update_path, UPDATE_SHA256)
    base_iv = decoder[0x3E20:0x3E30]
    base_key = decoder[0x3E30:0x3E40]
    key = decrypt(decoder[0x3E40:0x3E50], base_key, base_iv)
    iv = decrypt(decoder[0x3E50:0x3E60], base_key, base_iv)
    header = decrypt(update[:256], key, iv)
    expected_crc = struct.unpack_from(">I", header, 252)[0]
    if header[:4] != b"HEVC" or zlib.crc32(header[:252]) != expected_crc:
        raise ValueError("更新ヘッダーの識別子またはCRCが一致しません")

    entries: list[tuple[int, int, int, int, bytes]] = []
    seen: set[int] = set()
    ranges: list[tuple[int, int]] = []
    for index in range(11):
        kind, flags, table = struct.unpack_from(">BBH", header, 32 + 4 * index)
        if kind == 0:
            continue
        if kind not in ENCODINGS or kind in seen or flags != 0:
            raise ValueError("未照合の領域定義です")
        if table < 76 or table > 236 or table % 4:
            raise ValueError("領域定義の位置が範囲外です")
        offset, length, crc, padded = struct.unpack_from(">4I", header, table)
        if not 0 < length <= padded or padded % 16 or offset < 256:
            raise ValueError("領域の長さまたは位置が不正です")
        end = offset + padded
        if end > len(update) or any(offset < right and left < end for left, right in ranges):
            raise ValueError("領域が重複しているか、ファイルの末尾を超えています")
        data = update[offset:end]
        if ENCODINGS[kind] == "AES-128-CBC":
            data = decrypt(data, key, iv)
        data = data[:length]
        if zlib.crc32(data) != crc:
            raise ValueError(f"領域{kind}のCRCが一致しません")
        entries.append((kind, offset, padded, crc, data))
        ranges.append((offset, end))
        seen.add(kind)
    if seen != set(ENCODINGS):
        raise ValueError("照合済みの6領域が揃っていません")

    output.mkdir(mode=0o700)
    write_new(output / "header.bin", header)
    manifest = []
    for kind, offset, padded, crc, data in entries:
        write_new(output / f"module-{kind}.bin", data)
        manifest.append({
            "type": kind,
            "offset": offset,
            "length": len(data),
            "padded_length": padded,
            "encoding": ENCODINGS[kind],
            "crc32": f"{crc:08x}",
            "sha256": hashlib.sha256(data).hexdigest(),
        })
    report = {
        "decoder_sha256": DECODER_SHA256,
        "update_sha256": UPDATE_SHA256,
        "header_crc32": f"{expected_crc:08x}",
        "modules": manifest,
    }
    write_new(output / "manifest.json", (json.dumps(report, indent=2) + "\n").encode())
    print(f"6領域のCRCを照合して保存しました: {output}")


def main() -> None:
    parser = argparse.ArgumentParser(description="照合済みBS1909122を親PC上で展開します")
    parser.add_argument("decoder", type=Path, help="保存済みの/etc/dec_local")
    parser.add_argument("update", type=Path, help="公式のHD30_134.RVP")
    parser.add_argument("output", type=Path, help="新規作成する保存先")
    args = parser.parse_args()
    extract(args.decoder, args.update, args.output)


if __name__ == "__main__":
    main()
