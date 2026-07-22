#!/usr/bin/env bash
#
# Rsync the marketing site (marketing/) to the server. No build step —
# it's hand-authored static HTML/CSS, unlike scripts/deploy.sh which
# builds the Dioxus app first.
#
# Usage:
#   ./scripts/deploy-marketing.sh
#
# Config (env vars, or scripts/.env.deploy next to this script):
#   DEPLOY_HOST, DEPLOY_USER, DEPLOY_PORT — same as deploy.sh
#   MARKETING_DEPLOY_PATH — absolute path on the server to sync into

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ -f "$SCRIPT_DIR/.env.deploy" ]; then
    # shellcheck disable=SC1091
    source "$SCRIPT_DIR/.env.deploy"
fi

DEPLOY_USER="${DEPLOY_USER:-$(whoami)}"
DEPLOY_PORT="${DEPLOY_PORT:-22}"

SITE_SRC="$REPO_ROOT/marketing"

if [ ! -f "$SITE_SRC/index.html" ]; then
    echo "No $SITE_SRC/index.html found — nothing to deploy." >&2
    exit 1
fi

if [ -z "${DEPLOY_HOST:-}" ] || [ -z "${MARKETING_DEPLOY_PATH:-}" ]; then
    cat >&2 <<'EOF'
DEPLOY_HOST and/or MARKETING_DEPLOY_PATH aren't set. Either export them,
or add MARKETING_DEPLOY_PATH=... to scripts/.env.deploy next to DEPLOY_HOST.
EOF
    exit 1
fi

echo "==> Syncing $SITE_SRC to ${DEPLOY_USER}@${DEPLOY_HOST}:${MARKETING_DEPLOY_PATH}"
ssh -p "$DEPLOY_PORT" "${DEPLOY_USER}@${DEPLOY_HOST}" "mkdir -p ${MARKETING_DEPLOY_PATH}"
rsync -avz --delete \
    -e "ssh -p ${DEPLOY_PORT}" \
    "$SITE_SRC/" \
    "${DEPLOY_USER}@${DEPLOY_HOST}:${MARKETING_DEPLOY_PATH}/"

echo "==> Done."
