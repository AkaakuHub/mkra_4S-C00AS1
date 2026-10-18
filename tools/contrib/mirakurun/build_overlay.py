#!/usr/bin/env python3
from io import BytesIO
from pathlib import Path
import sys
import tarfile
from tempfile import TemporaryDirectory

from patch_runtime import main as patch_runtime


RUNTIME_DIR = "opt/mirakurun/node_modules/mirakurun/lib/Mirakurun"
NODE_MEMBER = "node-v22.23.3-linux-armv7l/bin/node"


def deb_data(path: Path) -> BytesIO:
    data = path.read_bytes()
    if not data.startswith(b"!<arch>\n"):
        raise ValueError("invalid Debian archive")
    offset = 8
    while offset < len(data):
        header = data[offset : offset + 60]
        name = header[:16].decode("ascii").strip().rstrip("/")
        size = int(header[48:58].decode("ascii"))
        offset += 60
        member = data[offset : offset + size]
        if name == "data.tar.xz":
            return BytesIO(member)
        offset += size + size % 2
    raise ValueError("Debian archive has no data.tar.xz")


def main(archives: Path, mkra: Path, output: Path, work: Path) -> None:
    with TemporaryDirectory(dir=work) as stage_name:
        stage = Path(stage_name)
        with tarfile.open(archives / "mirakurun-runtime.tar.gz", "r:gz") as runtime:
            for name in ("Channel.js", "common.js", "ServiceItem.js"):
                relative = f"{RUNTIME_DIR}/{name}"
                target = stage / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(runtime.extractfile(relative).read())
        patch_runtime(stage)

        with tarfile.open(output, "w:gz") as overlay:
            with tarfile.open(archives / "node-v22.23.3-linux-armv7l.tar.xz", "r:xz") as node:
                member = node.getmember(NODE_MEMBER)
                stream = node.extractfile(member)
                member.name = "usr/local/bin/node"
                overlay.addfile(member, stream)

            with tarfile.open(fileobj=deb_data(archives / "libatomic1_10.5.0-1ubuntu1~20.04_armhf.deb"), mode="r:xz") as atomic:
                for member in atomic:
                    member.name = member.name.removeprefix("./")
                    if member.name.startswith("usr/lib/arm-linux-gnueabihf/libatomic.so"):
                        overlay.addfile(member, atomic.extractfile(member) if member.isfile() else None)

            for name in ("Channel.js", "common.js", "ServiceItem.js"):
                relative = f"{RUNTIME_DIR}/{name}"
                overlay.add(stage / relative, arcname=relative)
            for name in ("server.yml", "tuners.yml", "channels.yml"):
                overlay.add(Path(__file__).parent / name, arcname=f"usr/local/etc/mirakurun/{name}")
            overlay.add(mkra, arcname="usr/local/bin/mkra")


if __name__ == "__main__":
    main(*(Path(value) for value in sys.argv[1:5]))
