import argparse
import re
import shlex
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from sharp_telnet import connect


def apply_modules(session, module_dir: Path, *, codec_bypass: bool, diagnostics: bool) -> None:
    names = ['freezer', 'display', 'wake', 'lan', 'front', 'mmc']
    if codec_bypass:
        names.append('codec')
    if diagnostics:
        names.append('diag')
    for name in names:
        path = module_dir / f'as1_{name}.ko'
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f'Module is missing or empty: {path}')

    if session.run('uname -r').strip() != '3.10.24':
        raise RuntimeError('Unexpected kernel version')
    pid = session.run('pidof abs', raw=True).strip()
    if not pid.isdecimal():
        raise RuntimeError('Expected one native abs process')
    digest = session.run(f'md5sum /proc/{pid}/exe').split()[0]
    if digest != '1a3fdfa8379d75a802190b48ea951e9a':
        raise RuntimeError('Unexpected native abs binary')
    if any(line.startswith('as1_') for line in session.run('cat /proc/modules').splitlines()):
        raise RuntimeError('Comparison modules are already loaded; use a fresh boot')
    threads = session.run(
        f'for d in /proc/{pid}/task/[0-9]*; do echo "$d $(cat "$d/comm")"; done',
        raw=True,
    )
    front: dict[str, str] = {}
    for line in threads.splitlines():
        path, _, name = line.partition(' ')
        if name in ('abs.T_FPD', 'abs.T_FPD3'):
            if name in front or not re.fullmatch(r'/proc/\d+/task/\d+', path):
                raise RuntimeError('Unexpected front-controller tasks')
            front[name] = path.rsplit('/', 1)[1]
    if len(front) != 2:
        raise RuntimeError('Both native front-controller tasks are required')

    for name in names:
        remote = f'/tmp/mt/as1_{name}.ko'
        digest = session.upload(module_dir / f'as1_{name}.ko', remote)
        result = session.run(f'insmod {shlex.quote(remote)} && echo loaded', raw=True)
        if result.strip() != 'loaded':
            raise RuntimeError(f'Module load failed: {name}: {result}')
        print(f'as1_{name}: {digest}', flush=True)
    commands = [
        'echo 1 > /sys/module/as1_freezer/parameters/enabled',
        'for d in /proc/[0-9]*/task/[0-9]*; do '
        'w=$(cat "$d/wchan" 2>/dev/null); '
        'if test "$w" = sleep_current_thread_on_interrupt; then '
        'echo "${d##*/}" > /sys/module/as1_freezer/parameters/wake_tid; fi; done',
        'echo N > /sys/module/printk/parameters/console_suspend',
        'echo 1 > /sys/module/as1_mmc/parameters/enabled',
        'echo 1 > /sys/module/as1_display/parameters/enabled',
    ]
    if codec_bypass:
        commands.append('echo 1 > /sys/module/as1_codec/parameters/bypass')
    for tid in front.values():
        commands.append(f'echo {tid} > /sys/module/as1_front/parameters/keep_tid')
    commands.extend([
        'echo 1 > /sys/module/as1_front/parameters/enabled',
        'echo 1 > /sys/module/as1_wake/parameters/enabled',
        'echo 1 > /sys/module/as1_lan/parameters/enabled',
        'echo none > /sys/power/pm_test',
    ])
    for command in commands:
        result = session.run(f'{command} && echo applied', raw=True, timeout=20)
        if result.strip() != 'applied':
            raise RuntimeError(f'Configuration failed: {command}: {result}')
    print(session.run(
        'cat /sys/module/as1_freezer/parameters/stats '
        '/sys/module/as1_front/parameters/stats '
        '/sys/module/as1_wake/parameters/stats '
        '/sys/module/as1_display/parameters/stats '
        '/sys/module/as1_mmc/parameters/stats '
        '/sys/module/as1_lan/parameters/stats',
    ))


def main() -> None:
    parser = argparse.ArgumentParser(description='AS1のRAMへ本体待機の比較用モジュールを適用します。')
    parser.add_argument('--host', required=True)
    parser.add_argument('--module-dir', type=Path, required=True)
    parser.add_argument('--codec-bypass', action='store_true')
    parser.add_argument('--diagnostics', action='store_true')
    args = parser.parse_args()
    session = connect(args.host, timeout=10)
    try:
        apply_modules(session, args.module_dir, codec_bypass=args.codec_bypass, diagnostics=args.diagnostics)
    finally:
        session.close()


if __name__ == '__main__':
    main()
