#!/usr/bin/env bash
# Launch filegram under a virtual X server. FILEGRAM_SMOKE (set in the image)
# exits after the first rendered frame; FILEGRAM_FUNCTIONAL_TEST instead drives
# the selected functional scenario. The timeout is a safety net: if the app neither
# renders nor crashes — e.g. it hangs waiting on a surface — fail loudly instead
# of blocking CI forever.
set -euo pipefail

if [[ "${FILEGRAM_FUNCTIONAL_TEST:-}" == "edge-cases" ]]; then
    mkdir -p /smoke-fixture/empty
    : > /smoke-fixture/zero.bin
    printf 'unicode' > '/smoke-fixture/файл-😀.txt'
    ln -s .. /smoke-fixture/sub/loop
fi

timeout --signal=KILL 60 \
    xvfb-run --auto-servernum --server-args="-screen 0 1024x768x24" \
    filegram "$@"

if [[ -n "${FILEGRAM_FUNCTIONAL_TEST:-}" ]]; then
    echo "functional GUI test passed"
else
    echo "smoke test passed: filegram started and rendered a frame"
fi
