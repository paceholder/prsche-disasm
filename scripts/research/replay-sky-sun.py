"""Record the original sun position: `Sky_SunPosition` 0x461090 (cdecl: offset a, offset b,
rotation, height, curvature, out*, scale), the call the lens-flare object 0x460780 makes with
the fields `+0xcdc`, `+0xce0`, `+0x3c`, `+0x48`, `+0x44`, `+0xce4` of `Sky/<track>.bin`.

Executes nfs5.exe in Unicorn (render_harness.RenderMachine; SinTurns 0x5327e0 / CosTurns
0x5327f0 run for real, FPU control word 0x027f as the game's). Writes `out` (x, y, z, w) as f32
bits.

- `--synthetic`: a fixed set of argument tuples (0 / quarter turns, negative offsets, large
  scales, random values); no game data, the fixture can be committed.
- `--tracks`: the six fields of every GameData/Track/Sky/*.bin (game data: local/fixtures).

    python scripts/research/replay-sky-sun.py --synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/sky-sun.tsv
    python scripts/research/replay-sky-sun.py --game-dir local/game --tracks \
        --out local/fixtures/sky-sun-tracks.tsv
"""
import argparse
from pathlib import Path
import random
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import render_harness as rh  # noqa: E402

SUN_POSITION = 0x461090
OUT = rh.DATA + 0x100


def u32f(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def run(m, args):
    a, b, rot, height, curv, scale = args
    m.put_bytes(OUT, bytes(16))
    m.call(SUN_POSITION, a, b, rot, height, curv, OUT, scale)
    return struct.unpack('<4I', m.get_bytes(OUT, 16))


def synthetic():
    rnd = random.Random(0x461090)
    out = [(0.0, 0.0, 0.0, 125.0, 0.0002, 0.1), (2.0, 3.0, 0.25, 10.0, 0.0, 1.0),
           (-5.5, 7.25, 0.62, 44.4, 0.0002, 0.236), (1500.0, -750.0, 0.999, 125.0, 0.0002, 1.5),
           (0.0, 0.0, -0.25, 75.0, 0.0004, 0.1), (100.0, 100.0, 0.5, 200.0, 0.001, 3.0)]
    for _ in range(60):
        out.append((rnd.uniform(-800, 800), rnd.uniform(-800, 800), rnd.uniform(-1.5, 1.5),
                    rnd.uniform(0, 300), rnd.uniform(0, 0.002), rnd.uniform(0.01, 4)))
    return [(f'synthetic-{i}', tuple(u32f(v) for v in t)) for i, t in enumerate(out)]


def tracks(game_dir):
    out = []
    for p in sorted((game_dir / 'GameData/Track/Sky').iterdir(), key=lambda p: p.name.lower()):
        if p.suffix.lower() != '.bin':
            continue
        d = p.read_bytes()
        if len(d) < 0xce8 or struct.unpack_from('<I', d, 0)[0] < 6:
            continue
        f = lambda o: struct.unpack_from('<I', d, o)[0]  # noqa: E731
        out.append((p.stem.lower(), (f(0xcdc), f(0xce0), f(0x3c), f(0x48), f(0x44), f(0xce4))))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--game-dir', type=Path, default=None)
    ap.add_argument('--out', type=Path, required=True)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument('--synthetic', action='store_true')
    g.add_argument('--tracks', action='store_true')
    args = ap.parse_args()
    m = rh.RenderMachine(args.game_dir)
    cases = synthetic() if args.synthetic else tracks(args.game_dir or rh.ROOT / 'local/game')
    lines = ['# replay-sky-sun.py: nfs5.exe 0x461090 Sky_SunPosition (a, b, rotation, height, '
             'curvature, scale -> x, y, z, w; f32 bits)', 'case\targs\tout']
    for name, a in cases:
        r = run(m, a)
        lines.append(f'{name}\t{",".join(f"{v:08x}" for v in a)}\t{",".join(f"{v:08x}" for v in r)}')
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{args.out}: {len(cases)} rows')


if __name__ == '__main__':
    main()
