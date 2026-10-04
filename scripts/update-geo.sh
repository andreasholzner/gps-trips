#!/usr/bin/env bash
# Refresh the place and ground databases (US-74/US-76, ADR-0027): fetch the
# sources that changed, rebuild, and optionally put the result on the volume.
# The steps are the ones docs/deployment.md describes by hand.
#
# usage: scripts/update-geo.sh [--no-download] [--upload] [places|ground]...
#   --no-download  build from what is already in data/geo-src/
#   --upload       copy the built databases to the Fly volume (needs FLY_APP)
#   places, ground which databases to rebuild (default: both)
set -euo pipefail

cd "$(dirname "$0")/.."

usage() { sed -n '6,9s/^# \{0,1\}//p' "$0" >&2; exit 2; }

download=1 upload=0 dbs=()
for arg in "$@"; do
    case "$arg" in
        --no-download) download=0 ;;
        --upload) upload=1 ;;
        places | ground) dbs+=("$arg") ;;
        *) usage ;;
    esac
done
(( ${#dbs[@]} )) || dbs=(places ground)
wants() { [[ " ${dbs[*]} " == *" $1 "* ]]; }

for tool in osmium curl unzip; do
    command -v "$tool" >/dev/null || { echo "$tool is needed." >&2; exit 1; }
done
if (( upload )); then
    : "${FLY_APP:?set FLY_APP to the Fly app name}"
    command -v fly >/dev/null || { echo "fly is needed for --upload." >&2; exit 1; }
fi

src=data/geo-src
mkdir -p "$src"

geofabrik=https://download.geofabrik.de/europe
extracts=(norway sweden finland germany austria switzerland slovenia
    italy/nord-ovest italy/nord-est france/rhone-alpes france/provence-alpes-cote-d-azur)
kartverket=https://nedlasting.geonorge.no/geonorge/Basisdata/Stedsnavn/GML/Basisdata_0000_Norge_4258_Stedsnavn_GML.zip
sea=https://osmdata.openstreetmap.de/download/water-polygons-split-4326.zip

gml=$src/Basisdata_0000_Norge_4258_Stedsnavn_GML.gml
shp=$src/water-polygons-split-4326/water_polygons.shp

# Fetches $1 into $2 unless the copy there is as new as the server's. The
# download goes to a side file, so an interrupted one never passes for whole.
fetch() {
    local url=$1 dest=$2 part=$2.part
    local since=()
    [[ -f "$dest" ]] && since=(-z "$dest")
    rm -f "$part"
    echo "Checking $url"
    curl -fL --retry 3 --remote-time "${since[@]}" -o "$part" "$url"
    if [[ -f "$part" ]]; then mv "$part" "$dest"; else echo "  unchanged"; fi
}

# Unpacks archive $1 when $2, a file inside it, is missing or older. The
# extracted file is touched, since unzip gives it the archive's own date.
unpack() {
    local zip=$1 file=$2
    if [[ ! -f "$file" || "$zip" -nt "$file" ]]; then
        echo "Unpacking $zip"
        unzip -o -q "$zip" -d "$src"
        touch "$file"
    fi
}

if (( download )); then
    for extract in "${extracts[@]}"; do
        fetch "$geofabrik/$extract-latest.osm.pbf" "$src/${extract##*/}-latest.osm.pbf"
    done
    if wants places; then fetch "$kartverket" "$src/${kartverket##*/}"; fi
    if wants ground; then fetch "$sea" "$src/${sea##*/}"; fi
fi
if wants places; then unpack "$src/${kartverket##*/}" "$gml"; fi
if wants ground; then unpack "$src/${sea##*/}" "$shp"; fi

pbfs=()
for extract in "${extracts[@]}"; do
    pbf=$src/${extract##*/}-latest.osm.pbf
    [[ -f "$pbf" ]] || { echo "$pbf is missing." >&2; exit 1; }
    pbfs+=("$pbf")
done

cargo build --release --bin places_build
# places_build refuses to write into an existing database, so each is built
# beside the old one and swapped in only once it is whole.
build() {
    local db=$1; shift
    rm -f "data/$db.sqlite.new"
    echo "Building data/$db.sqlite"
    target/release/places_build "$@"
    mv "data/$db.sqlite.new" "data/$db.sqlite"
}
if wants places; then build places build data/places.sqlite.new "${pbfs[@]}" "$gml"; fi
if wants ground; then build ground ground data/ground.sqlite.new "${pbfs[@]}" "$shp"; fi

(( upload )) || exit 0

# Neither sftp nor ssh counts as traffic, so the machine is kept up while the
# files travel; the restart that turns auto-stop back on makes the server
# open the new files.
machine=$(fly machines list --app "$FLY_APP" --quiet)
fly machine update "$machine" --autostop=off --app "$FLY_APP" --yes
trap 'fly machine update "$machine" --autostop=stop --app "$FLY_APP" --yes' EXIT
for db in "${dbs[@]}"; do
    echo "Uploading data/$db.sqlite"
    fly ssh console --app "$FLY_APP" -C "rm -f /data/$db.sqlite.new"
    fly ssh sftp put "data/$db.sqlite" "/data/$db.sqlite.new" --app "$FLY_APP"
    fly ssh console --app "$FLY_APP" -C "mv /data/$db.sqlite.new /data/$db.sqlite"
done
fly machine update "$machine" --autostop=stop --app "$FLY_APP" --yes
trap - EXIT
fly logs --app "$FLY_APP" --no-tail | grep 'Suggesting' || true
