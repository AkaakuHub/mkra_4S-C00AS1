#!/usr/bin/env python3
from pathlib import Path
import sys


def replace_once(path: Path, before: str, after: str) -> None:
    source = path.read_text(encoding="utf-8")
    if source.count(before) != 1:
        raise ValueError(f"expected one matching expression in {path}")
    path.write_text(source.replace(before, after), encoding="utf-8")


def main(root: Path) -> None:
    base = root / "opt/mirakurun/node_modules/mirakurun/lib/Mirakurun"
    replace_once(
        base / "Channel.js",
        'channel.type !== "CS" && channel.type !== "SKY"',
        'channel.type !== "CS" && channel.type !== "SKY" && channel.type !== "BS4K"',
    )
    replace_once(
        base / "common.js",
        'exports.channelTypes = ["GR", "BS", "CS", "SKY"];',
        'exports.channelTypes = ["GR", "BS", "CS", "SKY", "BS4K"];',
    )
    replace_once(
        base / "ServiceItem.js",
        '            case "BS":\n                order = "2";',
        '            case "BS":\n            case "BS4K":\n                order = "2";',
    )


if __name__ == "__main__":
    main(Path(sys.argv[1]))
