#!/bin/bash
#
# Thin wrapper kept for muscle-memory (`./build.sh`). The app is
# frontend-only now — no compiled server binary, no process to restart —
# so this is just scripts/deploy.sh: build static web output, rsync it.
# Server host/path live in scripts/.env.deploy (gitignored), not here.
set -e
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/scripts/deploy.sh" "$@"
