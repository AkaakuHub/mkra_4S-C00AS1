#!/bin/sh
set -eu

mkra=$1
sleep 60
"$mkra" fem-dev --path 1 --dev 0x200
sleep 1
"$mkra" tune4k --sid 101 --tune-mode 3
