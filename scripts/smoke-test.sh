#!/usr/bin/env bash
# Build the headless smoke-test image and run it. Used both locally
# (`./scripts/smoke-test.sh`) and by the smoke-test CI job.
set -euo pipefail
cd "$(dirname "$0")/.."

docker build -f docker/smoke.Dockerfile -t filegram-smoke .
docker run --rm filegram-smoke
