#!/usr/bin/env python3
"""Read a W25Q16JV with JEDEC EF 70 15 via UNO R4 Minima and pyserial.

Usage: py -m pip install pyserial
       py read_as1_windows.py COM5
"""
import argparse
import hashlib
import pathlib
import sys
import time

from progress import Progress

try:
    import serial
except ImportError:
    sys.exit("pyserial がありません: py -m pip install pyserial")

SIZE = 2 * 1024 * 1024
BLOCK = 256
EXPECTED_JEDEC = bytes.fromhex("EF7015")


def read_exact(port, n):
    data = bytearray()
    while len(data) < n:
        part = port.read(n - len(data))
        if not part:
            raise RuntimeError(f"USB通信がタイムアウトしました ({len(data)}/{n} bytes)")
        data.extend(part)
    return bytes(data)


def read_full(port, name, pass_number):
    outfile = pathlib.Path(name)
    if outfile.exists():
        raise FileExistsError(f"既存ファイルを上書きしません: {outfile}")
    sha = hashlib.sha256()
    progress = Progress(f"ROM読み出し {pass_number}/2", SIZE)
    try:
        with outfile.open("xb") as f:
            for address in range(0, SIZE, BLOCK):
                port.write(b"R" + address.to_bytes(3, "little"))
                block = read_exact(port, BLOCK)
                f.write(block)
                sha.update(block)
                progress.update(address + BLOCK)
    except Exception:
        print(f"読み出し失敗。{outfile} は不完全な可能性があります。", file=sys.stderr)
        raise
    print(f"{name}: SHA256={sha.hexdigest()}")
    return sha.digest()


def main():
    parser = argparse.ArgumentParser(description="SPI ROMを2回読み出して比較します。")
    parser.add_argument("port", help="COM3などのシリアルポート")
    parser.add_argument("--probe", action="store_true", help="試験読み出しだけ実行します")
    args = parser.parse_args()
    com = args.port
    filenames = ("as1-original-1.bin", "as1-original-2.bin")
    if not args.probe:
        for filename in filenames:
            if pathlib.Path(filename).exists():
                raise FileExistsError(f"既存ファイルを上書きしません: {filename}")
    print(f"接続: {com}")
    if not args.probe:
        print("ROMを2回読み出します。完了後に全容量のSHA-256を比較します。", flush=True)
    with serial.Serial(com, baudrate=115200, timeout=30, write_timeout=30) as port:
        time.sleep(2.0)
        port.reset_input_buffer()
        # Verify that the ID is stable before any bulk reads.
        for attempt in range(1, 6):
            port.write(b"I")
            ident = read_exact(port, 3)
            print(f"JEDEC ID ({attempt}/5): {ident.hex(' ').upper()}", flush=True)
            if ident != EXPECTED_JEDEC:
                raise RuntimeError("JEDEC ID が EF 70 15 と一致しません。読み出しを中止します。")
        # Check a few read locations twice, to catch unstable data-path signals.
        for address in (0x000000, 0x010000, 0x1FF000):
            samples = []
            for _ in range(2):
                port.write(b"R" + address.to_bytes(3, "little"))
                samples.append(read_exact(port, BLOCK))
            if samples[0] != samples[1]:
                differences = [
                    offset
                    for offset, (first, second) in enumerate(zip(*samples))
                    if first != second
                ]
                print(
                    f"不一致: {len(differences)}/{BLOCK} バイト",
                    file=sys.stderr,
                )
                for offset in differences[:16]:
                    first, second = samples[0][offset], samples[1][offset]
                    print(
                        f"  0x{address + offset:06X}: "
                        f"1回目={first:02X} 2回目={second:02X} "
                        f"XOR={first ^ second:02X}",
                        file=sys.stderr,
                    )
                raise RuntimeError(f"読み出しが不安定です。アドレス {address:06X} の2回の結果が異なります。")
            print(f"試験読み出し OK: 0x{address:06X}", flush=True)
        if args.probe:
            return
        hashes = []
        for pass_number, filename in enumerate(filenames, start=1):
            hashes.append(read_full(port, filename, pass_number))
        if hashes[0] != hashes[1]:
            raise RuntimeError("2回の読み出し結果が一致しません。書き込みは絶対に行わないでください。")
        # Identical hashes are a strong equality check; ensure neither all-00 nor all-FF.
        original = pathlib.Path("as1-original-1.bin").read_bytes()
        if original.count(b"\xff") == SIZE or original.count(b"\x00") == SIZE:
            raise RuntimeError("全FF/全00の不正データです。バックアップとして使用しないでください。")
        print("成功: 2,097,152 バイト × 2回 / SHA-256 一致")
        print("as1-original-1.bin を安全に保存してください。")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as exc:
        sys.exit(f"エラー: {exc}")
