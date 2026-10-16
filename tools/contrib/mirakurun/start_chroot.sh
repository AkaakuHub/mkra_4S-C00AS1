#!/bin/sh
set -eu

ROOT=/tmp/mt/ubuntu
test -x "$ROOT/usr/local/bin/node"
test -r "$ROOT/opt/mirakurun/node_modules/mirakurun/lib/server.js"
test -r "$ROOT/usr/local/etc/mirakurun/server.yml"
test -r "$ROOT/usr/local/etc/mirakurun/tuners.yml"
test -r "$ROOT/usr/local/etc/mirakurun/channels.yml"
test -r "$ROOT/proc/self/stat"

exec /usr/sbin/chroot "$ROOT" /bin/sh -c '
    export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
    cd /opt/mirakurun/node_modules/mirakurun
    exec /usr/local/bin/node --max-old-space-size=64 --max-semi-space-size=4 lib/server.js
'
