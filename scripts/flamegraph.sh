#!/usr/bin/env bash
# Génère un flamegraph SVG de examples/profile.rs, sans droits root.
#
# Utilise le profileur intégré à macOS (/usr/bin/sample) et inferno :
#   cargo install inferno rustfilt
#
# Usage : ./scripts/flamegraph.sh [sortie.svg] [durée_en_secondes]
set -euo pipefail

OUT="${1:-flamegraph.svg}"
DURATION="${2:-3}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cargo build --release --example profile
./target/release/examples/profile 1000000 > /dev/null &
PID=$!
sleep 0.5
/usr/bin/sample "$PID" "$DURATION" 1 -file "$TMP/sample.txt" > /dev/null 2>&1
kill "$PID" 2> /dev/null || true

inferno-collapse-sample "$TMP/sample.txt" | rustfilt > "$TMP/folded.txt"
inferno-flamegraph --title "mini-EVM : boucle de 10 000 tours" "$TMP/folded.txt" > "$OUT"
echo "flamegraph écrit dans $OUT"
