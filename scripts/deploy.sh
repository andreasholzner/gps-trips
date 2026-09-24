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

# A snapshot of the volume before every deploy (US-50). Migrations run on
# boot, so rolling an image back past one needs the volume as it was before.
# It is taken while the old machine runs: crash-consistent, which SQLite's
# WAL recovers from as from a power cut. No attached volume — the very first
# deploy, or one right after a restore — means nothing to keep yet.
command -v jq >/dev/null || { echo "Refusing to deploy: jq is needed to read flyctl's JSON." >&2; exit 1; }
volume=$(fly volumes list --app "$FLY_APP" --json | jq -r '.[] | select(.attached_machine_id != null) | .id')
if [[ -n "$volume" ]]; then
    snapshots() { fly volumes snapshots list "$volume" --app "$FLY_APP" --json; }
    # `snapshots create` only schedules one and names no id, so the new
    # snapshot is the one that was not there before.
    known=$(snapshots | jq -c '[(. // [])[].id]')
    fly volumes snapshots create "$volume" --app "$FLY_APP"
    for (( waited = 0; ; waited += 10 )); do
        status=$(snapshots | jq -r --argjson known "$known" \
            '[(. // [])[] | select(.id as $id | $known | index($id) | not)] | last | .status // "waiting"')
        [[ "$status" == created ]] && break
        if [[ "$status" == failed ]] || (( waited >= 600 )); then
            echo "Refusing to deploy: the snapshot of $volume is $status." >&2
            exit 1
        fi
        sleep 10
    done
    echo "Snapshot of $volume created."
fi

# The version the trip list shows (US-68): the commit's date in UTC and its
# short hash. Worked out here because the image is built without `.git`;
# the clean-tree check above is what makes it name what is deployed.
version="$(TZ=UTC git log -1 --format=%cd --date=format-local:%Y-%m-%d) · $(git rev-parse --short HEAD)"

exec fly deploy --app "$FLY_APP" --ha=false --remote-only \
    --build-arg "TRIP_ARCHIVE_VERSION=$version"
