#!/usr/bin/env bash
# Regenerates the game-derived test fixtures from YOUR OWN copy of the game.
#
# The repository contains no data taken from the original game. Tests that compare this port
# with the original (SIM-derived car specs, AI track graphs, engine audio, cameras) replay
# recordings made by executing the original nfs5.exe in Unicorn against game files. The
# recordings are written to the gitignored local/fixtures/ and the tests skip when they are
# absent.
#
# Usage: scripts/research/record-local-fixtures.sh [--check]
#   GAME_DIR  game install (default: <repo>/local/game, must contain nfs5.exe and GameData/)
#   PYTHON    Python 3 with `unicorn==2.1.3` installed (default: python3)
#   --check   also compare against scripts/research/local-fixtures.sha256 (hashes of the data the
#             tests were verified with); a mismatch means the recorder or the game files differ
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GAME_DIR="${GAME_DIR:-$ROOT/local/game}"
PYTHON="${PYTHON:-python3}"
OUT="$ROOT/local/fixtures"
HASHES="$ROOT/scripts/research/local-fixtures.sha256"
R="$ROOT/scripts/research"

[ -f "$GAME_DIR/nfs5.exe" ] || { echo "nfs5.exe not found in $GAME_DIR (set GAME_DIR)" >&2; exit 1; }
"$PYTHON" -c 'import unicorn' 2>/dev/null || { echo "$PYTHON needs the 'unicorn' package (pip install unicorn==2.1.3); set PYTHON" >&2; exit 1; }
mkdir -p "$OUT"

run() { echo "== $1"; "$PYTHON" "$R/$1" --game-dir "$GAME_DIR" "${@:2}"; }

run replay-vehicle-sim.py --out "$OUT/vehicle-sim.tsv"
for s in tyre engine controls step ground keyboard; do run "replay-vehicle-$s.py" --out-dir "$OUT"; done
run replay-camera.py --out-dir "$OUT"
run replay-engine-audio.py --out-dir "$OUT"
run replay-ai-track.py --out "$OUT/ai-track-graph.tsv"
run replay-ai-query.py --out "$OUT/ai-track-query.tsv"
run replay-ai-ais.py --out "$OUT/ai-ais.tsv"
run replay-ai-planner.py --out "$OUT/ai-planner.tsv"
run replay-ai-loop.py --out "$OUT/ai-loop.tsv"

# Road support: dump the type-1 support polygons with the Rust loader, then record the original.
# Needs a Rust toolchain (cargo) on PATH.
SUPPORT_TRACKS="autobahn alps monaco1 canyon"
mkdir -p "$ROOT/local/derived/support"
echo "== support_dump"
(cd "$ROOT/iterations/012-campaign-fidelity" && cargo run --release -q -p nfs-assets --example support_dump -- \
  "$GAME_DIR" "$ROOT/local/derived/support" $SUPPORT_TRACKS)
run replay-road-support.py --polygons "$ROOT/local/derived/support" --out-dir "$OUT"

echo "wrote $(ls "$OUT" | wc -l | tr -d ' ') files to $OUT"
if [ "${1:-}" = "--check" ]; then
  [ -f "$HASHES" ] || { echo "no $HASHES" >&2; exit 1; }
  (cd "$OUT" && shasum -a 256 -c "$HASHES")
fi
