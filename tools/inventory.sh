#!/usr/bin/env bash
# Short reports on the code base, to keep it small: how big it is, what is
# copied, what the tests run, what is not used. A report prints a short
# summary; the full output stays in target/inventory, to be read only when
# the summary is not enough.
#
#   tools/inventory.sh size [rev]      lines per crate; with a rev, the change since it
#   tools/inventory.sh files [n]       the n largest files by code
#   tools/inventory.sh clones [tokens] copied code in rust and lua, the largest first
#   tools/inventory.sh cover [n]       what the tests run: per crate, and the
#                                      n largest functions no test runs
#   tools/inventory.sh deps            dependencies nobody uses
#   tools/inventory.sh map <package> [options]
#                                      the tree of modules and items of a package;
#                                      the options of `cargo modules structure`,
#                                      `--bin zyterm --max-depth 1` for the binary
#
# The tools come from /opt/packages.sh.

set -euo pipefail

cd "$(dirname "$0")/.."
. /opt/env.sh
OUT=target/inventory
mkdir -p "$OUT"
HERE=$(dirname "$0")

size() {
    if [ $# -gt 0 ]; then
        rustloc diff "$1" --by-crate --number-fmt=false
    else
        rustloc --by-crate --number-fmt=false
    fi
}

files() {
    rustloc --by-file --number-fmt=false -o code --top "${1:-30}"
}

clones() {
    local tokens=${1:-100} lang
    for lang in rust lua; do
        pmd cpd --minimum-tokens "$tokens" --language "$lang" --format csv_with_linecount_per_file \
            --dir src --dir crates --dir experiments --dir scripts --dir tests \
            --no-fail-on-violation >"$OUT/clones-$lang.csv" 2>"$OUT/clones-$lang.log" || true
    done
    python3 -I "$HERE/inventory/clones.py" "$OUT/clones-rust.csv" "$OUT/clones-lua.csv"
}

cover() {
    cargo llvm-cov --workspace --json --output-path "$OUT/cover.json" >"$OUT/cover.log" 2>&1 \
        || { tail -30 "$OUT/cover.log"; exit 1; }
    python3 -I "$HERE/inventory/cover.py" "$OUT/cover.json" "${1:-40}"
}

deps() {
    cargo shear --format=github
}

map() {
    [ $# -gt 0 ] || { echo "say which package: map <package> [options of cargo modules]"; exit 1; }
    local package=$1
    shift
    cargo modules structure --package "$package" "$@" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g'
}

case "${1:-}" in
    size | files | clones | cover | deps | map)
        command=$1
        shift
        "$command" "$@"
        ;;
    *)
        sed -n '2,/^$/s/^# \{0,1\}//p' "$0"
        exit 1
        ;;
esac
