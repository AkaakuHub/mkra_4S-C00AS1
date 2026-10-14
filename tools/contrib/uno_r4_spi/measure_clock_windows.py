#!/usr/bin/env python3
"""UNO R4のA1でSPI CLKのADC値を測定する。"""
import argparse
import hashlib
import struct
import sys
import time

from read_as1_windows import BLOCK, EXPECTED_JEDEC, SIZE, read_exact, serial


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("port", help="Arduinoのシリアルポート")
    parser.add_argument("--idle", action="store_true", help="CLKをHi-Zにした状態で測定")
    parser.add_argument("--address", type=lambda value: int(value, 0), default=0x1FF000, help="読み出すアドレス。0x表記も使用可能")
    parser.add_argument("--attempts", type=int, default=5, help="測定回数")
    args = parser.parse_args()
    if not 0 <= args.address <= SIZE - BLOCK:
        parser.error("アドレスが256バイトの読み出し範囲外です。")
    if args.attempts < 1:
        parser.error("測定回数は1以上にしてください。")
    if args.idle and args.address != 0x1FF000:
        parser.error("--idleでは--addressを指定しません。")

    print("Arduino A1をROMのCLK信号線に接続した配線で使用してください。", flush=True)
    first = None
    with serial.Serial(args.port, baudrate=115200, timeout=30, write_timeout=30) as port:
        time.sleep(2)
        port.reset_input_buffer()
        port.write(b"I")
        if read_exact(port, 3) != EXPECTED_JEDEC:
            raise RuntimeError("JEDEC IDがEF 70 15と一致しません。")
        for attempt in range(1, args.attempts + 1):
            if args.idle:
                port.write(b"B")
            else:
                port.write(b"C" + args.address.to_bytes(3, "little"))
                data = read_exact(port, BLOCK)
                if first is None:
                    first = data
                differences = sum(left != right for left, right in zip(first, data))
                print(f"測定{attempt}: SHA-256={hashlib.sha256(data).hexdigest()} 初回との差分={differences}バイト")
            levels = struct.unpack("<4096H", read_exact(port, 8192))
            if args.idle:
                print(f"測定{attempt}: Hi-Z ADC最小={min(levels)} 最大={max(levels)}", flush=True)
            else:
                low, high = levels[::2], levels[1::2]
                print(f"測定{attempt}: LOW ADC最小={min(low)} 最大={max(low)} / HIGH ADC最小={min(high)} 最大={max(high)}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as exc:
        sys.exit(f"エラー: {exc}")
