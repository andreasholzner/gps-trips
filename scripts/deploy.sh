#!/usr/bin/env bash
# Deploy the archive to Fly.io — the one command of US-45. See
# docs/deployment.md for the first-time setup this assumes.
set -euo pipefail

cd "$(dirname "$0")/.."

# The app name is the hostname, kept out of the (public) repository.
: "${FLY_APP:?set FLY_APP to the Fly app name}"

# `fly deploy` ships the directory as it is on disk, so only a clean tree
# guarantees that what runs is a commit.
if [[ -n "$(git status --porcelain)" ]]; then
    echo "Refusing to deploy: the working tree has uncommitted changes." >&2
    git status --short >&2
    exit 1
fi

# Exactly one machine. SQLite on a volume allows one writer, and a second
# machine would get a volume of its own — two archives drifting apart behind
# one address. `--ha=false` stops the deploy from creating one; this catches
# one added by other means.
machines=$(fly machines list --app "$FLY_APP" --quiet | grep -c . || true)
if (( machines > 1 )); then
    echo "Refusing to deploy: $FLY_APP has $machines machines, and must have exactly one." >&2
    exit 1
fi

exec fly deploy --app "$FLY_APP" --ha=false --remote-only
