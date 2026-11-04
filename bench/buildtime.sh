#!/bin/bash
# Usage: buildtime.sh [RUNS=3]
# Times building the synthetic app with each of 1, 10, 20, ..., 100
# components, with each library and profile: all of build.sh (rustc on the
# app crate, or with LTO the whole program; wasm-bindgen; wasm-opt; strip;
# brotli: -q 9 for the speed profile, -q 11 for size), with the dependencies
# already built.
#
# Each build is limited to 4 cores, and 4 run at a time, on separate
# physical cores (slot k: CPUs 4k..4k+3, which on the machine measured are
# physical cores, their hyperthreads left idle; adjust CPUS for another).
# Each run builds the 44 variants in a new random order, 4 at a time, so
# which run together varies. Writes every time (s) to
# out/buildtime-synthetic.json; plot with buildtime.mjs.
set -euo pipefail
cd "$(dirname "$0")"
RUNS=${1:-3}
CPUS=("0-3" "4-7" "8-11" "12-15")
COUNTS=(1 10 20 30 40 50 60 70 80 90 100)
export CARGO_BUILD_JOBS=4 BINARYEN_CORES=4

build() { # SLOT LIB PROFILE N: prints the seconds it took
    local start
    start=$(date +%s.%N)
    # (Compressing the speed profile's with brotli -q 9, not 11, so as not
    # to charge larger output much for compression time.)
    local brotli=11
    [ "$3" = speed ] && brotli=9
    SLOT=$1 BROTLI=$brotli taskset -c "${CPUS[$1]}" ./build.sh synthetic "$2" "$3" "$4" > /dev/null
    printf "%.3f\n" "$(echo "$(date +%s.%N) - $start" | bc)"
}

# Warm up each slot's target directories (the dependencies).
for slot in 0 1 2 3; do
    (for lib in yew leafy; do for profile in speed size; do build $slot $lib $profile 1 > /dev/null; done; done) &
done
wait
echo "warmed up"

configs=()
for n in "${COUNTS[@]}"; do for lib in yew leafy; do for profile in speed size; do configs+=("$lib $profile $n"); done; done; done
results=$(mktemp)
for run in $(seq "$RUNS"); do
    mapfile -t order < <(printf "%s\n" "${configs[@]}" | shuf --random-source=<(yes "$run"))
    for ((i = 0; i < ${#order[@]}; i += 4)); do
        for slot in 0 1 2 3; do
            [ $((i + slot)) -lt ${#order[@]} ] || continue
            read -r lib profile n <<< "${order[$((i + slot))]}"
            (echo "$lib $profile $n $(build $slot "$lib" "$profile" "$n")" >> "$results") &
        done
        wait
    done
    echo "run $run done"
done
# {"leafy": {"speed": {"1": [s, s, s], ...}, ...}, ...}
python3 - "$results" > out/buildtime-synthetic.json <<'PY'
import json, sys, collections
d = collections.defaultdict(lambda: collections.defaultdict(lambda: collections.defaultdict(list)))
for line in open(sys.argv[1]):
    lib, profile, n, s = line.split()
    d[lib][profile][n].append(float(s))
print(json.dumps(d, indent=1))
PY
rm "$results"
echo "out/buildtime-synthetic.json"
