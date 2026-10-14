#!/usr/bin/env python3
"""Risky: program ONLY the final 4 KiB U-Boot ENV sector after verified backups.

Usage:
    py write_telnet_sector_windows.py COM5 as1-original-1.bin telnet.bin --yes-write
This operation ERASES one 4 KiB sector. Interruptions may prevent the AS1 booting.
"""
import binascii
import hashlib
import pathlib
import sys
import time

from progress import Progress

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
import patch_bootargs

try:
    import serial
except ImportError:
    sys.exit("pyserial がありません: py -m pip install pyserial")

SIZE = 2 * 1024 * 1024
ENV_START = 0x1FF000
ENV_SIZE = 4096
CHUNK = 256


def read_exact(port, n):
    result = bytearray()
    while len(result) < n:
        new = port.read(n - len(result))
        if not new:
            raise RuntimeError(f"USBタイムアウト: {len(result)}/{n}")
        result.extend(new)
    return bytes(result)


def cmd_address(port, opcode, addr):
    port.write(opcode + addr.to_bytes(3, "little"))


def read_chunk(port, addr):
    cmd_address(port, b"R", addr)
    return read_exact(port, CHUNK)


def get_id(port):
    port.write(b"I")
    return read_exact(port, 3)


def digest(buf):
    return hashlib.sha256(buf).hexdigest()


def main():
    if len(sys.argv) != 5 or sys.argv[4] != "--yes-write":
        sys.exit("使い方: py write_telnet_sector_windows.py COM5 as1-original-1.bin telnet.bin --yes-write")

    comport, backup_name, patched_name = sys.argv[1:4]
    backup = pathlib.Path(backup_name).read_bytes()
    patched = pathlib.Path(patched_name).read_bytes()
    backup2 = pathlib.Path("as1-original-2.bin")
    if not backup2.exists() or backup2.read_bytes() != backup:
        sys.exit("2本目のバックアップ as1-original-2.bin がないか不一致です。中止します。")
    if len(backup) != SIZE or len(patched) != SIZE:
        sys.exit("サイズが2,097,152バイトではありません。中止します。")
    if backup[:ENV_START] != patched[:ENV_START]:
        sys.exit("最終4KiB以外に変更があります。中止します。")
    if backup[ENV_START:] == patched[ENV_START:]:
        sys.exit("最終4KiBに変更がありません。中止します。")
    for label, image in (("バックアップ", backup), ("変更イメージ", patched)):
        sector = image[ENV_START:]
        stored = int.from_bytes(sector[:4], "little")
        calculated = binascii.crc32(sector[4:]) & 0xFFFFFFFF
        if stored != calculated:
            sys.exit(f"{label}の環境CRCが不一致です。書き込みません。")
    entries = patch_bootargs.split_env(backup[ENV_START:])
    matches = [i for i, entry in enumerate(entries) if entry.startswith(b"set_bootargs=")]
    if len(matches) != 1:
        sys.exit("set_bootargsが一意ではありません。書き込みません。")
    index = matches[0]
    if entries[index] != b"set_bootargs=" + patch_bootargs.STOCK.encode("latin1"):
        sys.exit("set_bootargsが標準の値ではありません。書き込みません。")
    entries[index] = b"set_bootargs=" + patch_bootargs.TELNET.encode("latin1")
    if patched[ENV_START:] != patch_bootargs.build_env(entries):
        sys.exit("付属ツールのtelnet有効化パッチと一致しません。書き込みません。")

    print("バックアップ SHA256:", digest(backup))
    print("変更イメージ SHA256:", digest(patched))
    print("変更は最後の4KiBのみです。ここを消去すると途中で電源が切れた場合に起動不能になる可能性があります。")

    with serial.Serial(comport, 115200, timeout=30, write_timeout=30) as port:
        time.sleep(2)
        port.reset_input_buffer()
        for attempt in range(1, 6):
            ident = get_id(port)
            print(f"JEDEC ID ({attempt}/5):", ident.hex(" ").upper(), flush=True)
            if ident != bytes.fromhex("EF7015"):
                sys.exit("JEDEC IDが不正です。書き込みません。")

        print("ROM上の現環境セクターがバックアップと一致するか再確認します。")
        current_sector = b"".join(read_chunk(port, offset) for offset in range(ENV_START, SIZE, CHUNK))
        if current_sector != backup[ENV_START:]:
            sys.exit("ROM現物がバックアップと一致しません。書き込みません。")

        print("最終4KiBを消去します", flush=True)
        cmd_address(port, b"E", ENV_START)
        if read_exact(port, 1) != b"K":
            raise RuntimeError("消去コマンド失敗。AS1の電源を入れず、復旧を検討してください。")
        empty = b"".join(read_chunk(port, off) for off in range(ENV_START, SIZE, CHUNK))
        if empty != b"\xff" * ENV_SIZE:
            raise RuntimeError("消去検証に失敗。AS1の電源を入れず、復旧を検討してください。")
        print("消去検証OK。16ページを書き込みます。")

        for offset in range(ENV_START, SIZE, CHUNK):
            block = patched[offset:offset+CHUNK]
            port.write(b"P" + offset.to_bytes(3, "little") + block)
            if read_exact(port, 1) != b"K":
                raise RuntimeError(f"ページ書き込み失敗: 0x{offset:06X}")
            verify = read_chunk(port, offset)
            if verify != block:
                raise RuntimeError(f"ページ検証失敗: 0x{offset:06X}")
            print(f" 0x{offset:06X}: 書き込み・検証OK")

        print("ROM全体を再読出ししてtelnet.binと照合します")
        sha = hashlib.sha256()
        progress = Progress("ROM全体の書き込み検証", SIZE)
        for off in range(0, SIZE, CHUNK):
            block = read_chunk(port, off)
            if block != patched[off:off+CHUNK]:
                raise RuntimeError(f"全体検証不一致: 0x{off:06X}")
            sha.update(block)
            progress.update(off + CHUNK)
        print("完了。全2MiB検証一致: SHA256", sha.hexdigest())
        print("AS1のUSB/AC電源を入れる前にクリップを外してください。")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as e:
        sys.exit("エラー: " + str(e))
