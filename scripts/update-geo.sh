#!/usr/bin/env bash
# Refresh the place and ground databases (US-74/US-76, ADR-0027) on the Fly
# volume: fetch the sources that changed, rebuild the databases they make
# out of date, and put them on the volume. It says what it will do and asks
# first. The steps are the ones docs/deployment.md describes by hand.
#
# usage: scripts/update-geo.sh [--no-fetch] [--no-upload] [places|ground]...
#   --no-fetch     build from the sources in data/geo-src/, without checking for newer ones
#   --no-upload    keep the databases in data/, without copying them to the volume
#   places, ground which databases to update (default: both)
#   To rebuild a database that is up to date, delete it from data/ first.
set -euo pipefail

cd "$(dirname "$0")/.."

usage() { sed -n '7,11s/^# \{0,1\}//p' "$0" >&2; }

fetch=1 upload=1 dbs=()
for arg in "$@"; do
    case "$arg" in
        --no-fetch) fetch=0 ;;
        --no-upload) upload=0 ;;
        places | ground) dbs+=("$arg") ;;
        *) usage; exit 2 ;;
    esac
done
(( ${#dbs[@]} )) || dbs=(places ground)

needed=(osmium cargo unzip)
if (( fetch )); then needed+=(curl); fi
if (( upload )); then needed+=(fly jq); fi
for tool in "${needed[@]}"; do
    command -v "$tool" >/dev/null || { echo "$tool is needed." >&2; exit 1; }
done
if (( upload )); then : "${FLY_APP:?set FLY_APP to the Fly app name, or pass --no-upload}"; fi

src=data/geo-src
geofabrik=https://download.geofabrik.de/europe
extracts=(norway sweden finland germany austria switzerland slovenia
    italy/nord-ovest italy/nord-est france/rhone-alpes france/provence-alpes-cote-d-azur)
kartverket=https://nedlasting.geonorge.no/geonorge/Basisdata/Stedsnavn/GML/Basisdata_0000_Norge_4258_Stedsnavn_GML.zip
sea=https://osmdata.openstreetmap.de/download/water-polygons-split-4326.zip

gml=$src/Basisdata_0000_Norge_4258_Stedsnavn_GML.gml
shp=$src/water-polygons-split-4326/water_polygons.shp
builder=target/release/places_build

pbfs=()
for extract in "${extracts[@]}"; do pbfs+=("$src/${extract##*/}-latest.osm.pbf"); done

# The unzipped source database $1 is built from besides the extracts, the
# archive it comes out of, and where that is downloaded from.
own() { if [[ $1 == places ]]; then echo "$gml"; else echo "$shp"; fi; }
url() { if [[ $1 == places ]]; then echo "$kartverket"; else echo "$sea"; fi; }
archive() { local url; url=$(url "$1"); echo "$src/${url##*/}"; }
sources() { echo "${pbfs[@]}" "$(own "$1")"; }

# Why database $1 needs building, or nothing when it is up to date: it is
# missing, or older than a source. A change to how it is built is not
# seen: delete the database to have it rebuilt.
stale() {
    local db=data/$1.sqlite newer=()
    [[ -f "$db" ]] || { echo "data/$1.sqlite is missing"; return; }
    for file in $(sources "$1"); do
        [[ -f "$file" && ! "$file" -nt "$db" ]] || newer+=("${file##*/}")
    done
    if (( ${#newer[@]} )); then echo "older than ${newer[*]}"; fi
}

if (( ! fetch )); then
    for db in "${dbs[@]}"; do
        for file in $(sources "$db"); do
            [[ -f "$file" || ( $file == "$(own "$db")" && -f "$(archive "$db")" ) ]] ||
                { echo "$file is missing; run without --no-fetch." >&2; exit 1; }
        done
    done
fi

# What will happen, from what is here now.
work=0
echo
echo "This will:"
if (( fetch )); then
    missing=()
    for file in "${pbfs[@]}"; do [[ -f "$file" ]] || missing+=("${file##*/}"); done
    names=("the ${#extracts[@]} OpenStreetMap extracts")
    for db in "${dbs[@]}"; do
        names+=("$([[ $db == places ]] && echo "Kartverket's register" || echo "the sea")")
        [[ -f "$(archive "$db")" ]] || missing+=("$(basename "$(archive "$db")")")
    done
    last=${names[-1]}
    unset 'names[-1]'
    echo "  - check $(IFS=,; echo "${names[*]}" | sed 's/,/, /g') and $last for newer copies,"
    echo "    and download those that changed into $src/"
    (( ${#missing[@]} )) && echo "    (not here yet: ${missing[*]})"
    work=1
fi
for db in "${dbs[@]}"; do
    reason=$(stale "$db")
    if [[ -n "$reason" ]]; then
        echo "  - build data/$db.sqlite: $reason"
        work=1
    elif (( fetch )); then
        echo "  - build data/$db.sqlite only if a download brings a newer source (up to date now)"
    else
        echo "  - keep data/$db.sqlite: up to date"
    fi
done
if (( upload )); then
    echo "  - upload data/${dbs[0]}.sqlite${dbs[1]:+ and data/${dbs[1]}.sqlite} to the volume of $FLY_APP, keeping"
    echo "    its machine up meanwhile and restarting it after, so the server opens them"
    work=1
fi
if (( ! work )); then
    echo "  nothing: everything is up to date."
    exit 0
fi
echo
read -r -p "Go ahead? [y/N] " answer || answer=
if [[ $answer != [yY] ]]; then
    echo "Nothing done. To do less:" >&2
    usage
    exit 1
fi

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

# Neither sftp nor ssh counts as traffic, so the machine is kept up while the
# files travel; the restart that turns auto-stop back on makes the server
# open the new files.
upload_dbs() {
    # The JSON, since `--quiet` pads the id with blanks that `fly` then rejects.
    machine=$(fly machines list --app "$FLY_APP" --json | jq -r '.[0].id')
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
}

mkdir -p "$src"
if (( fetch )); then
    for extract in "${extracts[@]}"; do
        fetch "$geofabrik/$extract-latest.osm.pbf" "$src/${extract##*/}-latest.osm.pbf"
    done
    for db in "${dbs[@]}"; do fetch "$(url "$db")" "$(archive "$db")"; done
fi
for db in "${dbs[@]}"; do unpack "$(archive "$db")" "$(own "$db")"; done

cargo build --quiet --release --bin places_build
# places_build refuses to write into an existing database, so each is built
# beside the old one and swapped in only once it is whole.
for db in "${dbs[@]}"; do
    [[ -n "$(stale "$db")" ]] || { echo "data/$db.sqlite is up to date"; continue; }
    command=$([[ $db == places ]] && echo build || echo ground)
    rm -f "data/$db.sqlite.new"
    echo "Building data/$db.sqlite"
    # shellcheck disable=SC2046 # the sources are paths without blanks
    "$builder" "$command" "data/$db.sqlite.new" $(sources "$db")
    mv "data/$db.sqlite.new" "data/$db.sqlite"
done

if (( upload )); then upload_dbs; fi
