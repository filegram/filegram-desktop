#!/usr/bin/env bash
# Launch filegram under a virtual X server. FILEGRAM_SMOKE (set in the image)
# makes the app exit 0 as soon as the first frame renders. The timeout is a
# safety net: if the app neither renders nor crashes — e.g. it hangs waiting
# on a surface — fail loudly instead of blocking CI forever.
set -euo pipefail

timeout --signal=KILL 60 \
    xvfb-run --auto-servernum --server-args="-screen 0 1024x768x24" \
    filegram "$@"

echo "smoke test passed: filegram started and rendered a frame"
