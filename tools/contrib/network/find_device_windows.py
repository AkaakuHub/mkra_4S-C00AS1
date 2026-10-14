#!/usr/bin/env python3
"""指定したMACアドレスの機器を同じIPv4サブネットで探す。"""
import argparse
import ctypes
import ipaddress
import re
import sys
from concurrent.futures import ThreadPoolExecutor


def mac_address(value: str) -> bytes:
    if not re.fullmatch(r"[0-9a-fA-F]{12}|(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}|(?:[0-9a-fA-F]{2}-){5}[0-9a-fA-F]{2}", value):
        raise argparse.ArgumentTypeError("MACアドレスは12桁の16進数、またはコロン・ハイフン区切りで指定してください。")
    return bytes.fromhex(value.replace(":", "").replace("-", ""))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mac", type=mac_address, required=True, help="探す機器のMACアドレス")
    parser.add_argument("--source", type=ipaddress.IPv4Address, required=True, help="LANに接続したPCのIPv4アドレス")
    parser.add_argument("--network", type=ipaddress.IPv4Network, required=True, help="探索するサブネットをCIDR形式で指定")
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("このスクリプトはWindows用です。")
    if args.source not in args.network:
        parser.error("PCのIPv4アドレスが探索対象のサブネットに含まれていません。")

    send_arp = ctypes.WinDLL("iphlpapi").SendARP
    send_arp.argtypes = [ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
    send_arp.restype = ctypes.c_uint32
    source = int.from_bytes(args.source.packed, "little")

    def matches(address: ipaddress.IPv4Address) -> bool:
        buffer = (ctypes.c_uint32 * 2)()
        length = ctypes.c_uint32(ctypes.sizeof(buffer))
        result = send_arp(int.from_bytes(address.packed, "little"), source, buffer, ctypes.byref(length))
        if result == 1168:
            raise RuntimeError("指定したPCのIPv4アドレスに対応するネットワーク接続がありません。")
        return result == 0 and length.value == 6 and bytes(buffer)[:6] == args.mac

    addresses = [address for address in args.network.hosts() if address != args.source]
    print(f"{len(addresses)}個のIPv4アドレスを確認します。", file=sys.stderr, flush=True)
    found = False
    with ThreadPoolExecutor(max_workers=32) as pool:
        for address, match in zip(addresses, pool.map(matches, addresses)):
            if match:
                print(address, flush=True)
                found = True
    if not found:
        print("指定したMACアドレスの機器は見つかりませんでした。", file=sys.stderr)
    return 0 if found else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, ValueError) as exc:
        sys.exit(f"エラー: {exc}")
