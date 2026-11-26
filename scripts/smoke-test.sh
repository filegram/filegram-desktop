#!/usr/bin/env bash
# Build the headless GUI-test image and run both scenarios. Used locally via
# `./scripts/smoke-test.sh`; CI runs the same container commands separately.
set -euo pipefail
cd "$(dirname "$0")/.."

docker build -f docker/smoke.Dockerfile -t filegram-smoke .
docker run --rm filegram-smoke
docker run --rm -e FILEGRAM_FUNCTIONAL_TEST=1 filegram-smoke
