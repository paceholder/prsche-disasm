#!/usr/bin/env python3
"""Record the original AI car data load (change original-ai, task 3.1;
docs/re/ai.md section 4 in the workspace root).

Runs `Ais_Load` 0x49ca30 (with AI_TrackCornerFactor 0x4a2dc0,
AI_TrackLookFactor 0x4a2950 and AI_ModelClass 0x4a2bc0) in Unicorn for every
track x direction x installed .ais model, with two grip ratios car+0x10d8.
The file read 0x59d9b0 is stubbed to return the .ais bytes.

Columns: track, reverse, model, mu_10d8 (hex bits), sim140_in (hex bits) ->
ais_hex (0x130 bytes at car+0x764), sim140 (hex bits), width_5e6e34 (hex bits),
ranges_hex (0x5e74ac..0x5e7524: 10 (first, last) i32 pairs, then 10 f32
factors).
"""
import argparse
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ai_harness import AiMachine, load_module, ROOT  # noqa: E402
from vehicle_harness import CAR, RES, SPECS  # noqa: E402

TRACKS = ['alps', 'autobahn', 'canyon', 'castle', 'coastal', 'farmland', 'foothills', 'forest',
          'industrial', 'monaco1', 'monaco2', 'monaco3', 'monaco4', 'monaco5', 'skidpad']
AIS_LOAD = 0x49ca30
FILE_OPEN = 0x59d9b0
TRACK_NAME = 0x657444
REVERSE = 0x657430
AIS = SPECS + 0x400        # 0x130 bytes, inside the 0x1000 page after carSpecs
RES_MODEL = 0x2a8
TABLES = 0x5e6e34
RANGES = (0x5e74ac, 0x78)


def fbits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def record(game):
    module, data = load_module('nfs5.exe', game)
    ais_dir = game / 'GameData/Simulation/AICarData'
    models = sorted(p.stem for p in ais_dir.glob('*.ais'))
    m = AiMachine(module, data, stubs={FILE_OPEN: lambda mm: mm.file_addr})
    head = ['track', 'reverse', 'model', 'mu_10d8', 'sim140_in', 'ais_hex', 'sim140',
            'width_5e6e34', 'ranges_hex']
    rows = [head]
    for track in TRACKS:
        for reverse in (0, 1):
            for model in models:
                raw = (ais_dir / f'{model}.ais').read_bytes()
                for mu in (1.0, 0.9375):
                    m.reset_car()
                    m.put_u32(CAR + 0x764, AIS)
                    m.uc.mem_write(AIS, bytes(0x130))
                    m.put_str(RES + RES_MODEL, model, 0x40)
                    m.put_str(TRACK_NAME, track, 0x20)
                    m.put_u32(REVERSE, reverse)
                    m.put_u32(0x6afd80, 0)                     # _stricmp locale flag
                    m.put(CAR + 0x10d8, mu)
                    sim140 = 0.3
                    m.put(SPECS + 0x140, sim140)
                    m.uc.mem_write(TABLES, bytes(4))
                    m.uc.mem_write(RANGES[0], bytes(RANGES[1]))
                    m.files_top = 0x6000000
                    m.file_addr = m.place_file(raw)
                    m.call(AIS_LOAD, CAR, budget=5_000_000)
                    rows.append([track, str(reverse), model, f'{fbits(mu):08x}',
                                 f'{fbits(sim140):08x}', m.get_bytes(AIS, 0x130).hex(),
                                 f'{m.get_u32(SPECS + 0x140):08x}', f'{m.get_u32(TABLES):08x}',
                                 m.get_bytes(*RANGES).hex()])
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    p.add_argument('--out', type=Path, default=ROOT / 'local/fixtures/ai-ais.tsv')
    p.add_argument('--check', action='store_true', help='record twice and compare')
    a = p.parse_args()
    rows = record(a.game_dir)
    if a.check and record(a.game_dir) != rows:
        sys.exit('two recordings differ')
    head = ['# Original Ais_Load 0x49ca30 (0x4a2dc0, 0x4a2950, 0x4a2bc0) per track, direction,',
            '# model and grip ratio; recorded by scripts/research/replay-ai-ais.py.']
    a.out.write_text('\n'.join(head + ['\t'.join(r) for r in rows]) + '\n')
    print(f'{len(rows) - 1} rows', file=sys.stderr)


if __name__ == '__main__':
    main()
