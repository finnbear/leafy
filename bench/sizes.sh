#!/bin/bash
# Usage: sizes.sh APP...
# Builds each APP with each library and profile (build.sh), several at a
# time: 8 workers, each limited to an eighth of the CPUs (pinned to them).
# The yew builds go first, then (from each app's Cargo.lock, which they
# update, as build.sh does on its own) the leafy builds. In each, a
# profile's builds are split between 4 workers, each with a target
# directory of its own (so its dependencies are built once). Sizes don't
# depend on build speed, so this needn't run on an idle machine; output is
# in the order the builds finish.
set -euo pipefail
cd "$(dirname "$0")"
[ $# -gt 0 ] || { sed -n '2,10p' "$0" >&2; exit 1; }
APPS=("$@")
WORKERS=8
NCPU=$(nproc)
PER=$(( NCPU / WORKERS ))
[ "$PER" -ge 1 ] || PER=1
export CARGO_BUILD_JOBS=$PER BINARYEN_CORES=$PER

# Worker W's CPUs (wrapping, with fewer than 8).
cpus() {
    local first=$(( ($1 * PER) % NCPU ))
    echo "$first-$(( first + PER - 1 ))"
}

build_all() { # LIB
    local pids=() w=0
    for profile in speed size; do
        for slot in 0 1 2 3; do
            local c
            c=$(cpus $w)
            (
                i=0
                for app in "${APPS[@]}"; do
                    if [ $((i % 4)) = "$slot" ]; then
                        SLOT=p$slot taskset -c "$c" ./build.sh "$app" "$1" "$profile"
                    fi
                    i=$((i + 1))
                done
            ) &
            pids+=($!)
            w=$((w + 1))
        done
    done
    local failed=0
    for p in "${pids[@]}"; do
        wait "$p" || failed=1
    done
    return $failed
}

build_all yew
# (With a SLOT, build.sh leaves the app's Cargo.lock alone: updated here.)
for app in "${APPS[@]}"; do
    name=$app
    [ "$app" = synthetic ] && name=$app-200 # (build.sh's default N)
    cp "out/$name-yew-size/Cargo.lock" "apps/$app/Cargo.lock"
done
build_all leafy
