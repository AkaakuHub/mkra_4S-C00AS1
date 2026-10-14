import argparse
import ctypes
import os
from ctypes import wintypes
from pathlib import Path
import winreg

parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
directory = args.directory.resolve(strict=True)
video = directory / "LAVVideo.ax"
if not video.is_file():
    parser.error("LAVVideo.axがありません。")

api = ctypes.WinDLL("advapi32")
override = api.RegOverridePredefKey
override.argtypes = [wintypes.HKEY, wintypes.HKEY]
override.restype = wintypes.LONG
ole = ctypes.OleDLL("ole32")
ole.CoInitialize(None)
try:
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, r"Software\Classes") as key:
        with os.add_dll_directory(str(directory)):
            library = ctypes.WinDLL(str(video))
            register = library.DllRegisterServer
            register.argtypes = []
            register.restype = ctypes.c_long
            result = override(int(winreg.HKEY_CLASSES_ROOT), int(key))
            if result:
                raise ctypes.WinError(result)
            try:
                result = register()
                if result < 0:
                    raise OSError(f"DllRegisterServer failed: 0x{result & 0xffffffff:08X}")
                print("LAV Video Decoderを現在のユーザーに登録しました。")
            finally:
                result = override(int(winreg.HKEY_CLASSES_ROOT), None)
                if result:
                    raise ctypes.WinError(result)
finally:
    ole.CoUninitialize()
