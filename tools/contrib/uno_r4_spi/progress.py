import sys
import time


class Progress:
    def __init__(self, label, total):
        self.label = label
        self.total = total
        self.started = time.monotonic()
        self.updated = self.started

    def update(self, completed):
        now = time.monotonic()
        if completed != self.total and now - self.updated < 5:
            return
        self.updated = now
        elapsed = now - self.started
        speed = completed / elapsed if elapsed > 0 else 0
        remaining = int((self.total - completed) / speed) if speed else 0
        width = 24
        filled = completed * width // self.total
        bar = "#" * filled + "-" * (width - filled)
        line = (
            f"{self.label} [{bar}] {completed / self.total:6.1%} "
            f"{completed // 1024}/{self.total // 1024} KiB | "
            f"{speed / 1024:.1f} KiB/秒 | "
            f"残り約{remaining // 60}分{remaining % 60:02d}秒"
        )
        if sys.stdout.isatty():
            print("\r" + line, end="\n" if completed == self.total else "", flush=True)
        else:
            print(line, flush=True)
