#!/usr/bin/env python3
"""Record the original track node queries on every track (change original-ai,
task 2.3; docs/re/ai.md section 2 in the workspace root).

The graph is built by the original builder (ai_harness.py); then each query
runs on sampled inputs (seeded, deterministic):
  step      TrackNodes_Step 0x4817d0 (node, delta, branch) -> node
  ahead     TrackNodes_JunctionAhead 0x4816f0 (node, delta) -> junction | -1
  locate    TrackNodes_Locate 0x481400 (pos, hint) -> node
  walk      0x4816c0 (section, pos, hint) -> node
  locator   TrackLocator_Update 0x474440 on {node i16, section i32} (pos)
            -> node, changed flags +8/+9
  progress  TrackNodes_Progress 0x481b70 (node) -> start, end (f32 bits),
            length, first
Rows where the original faults (a branch slot holding builder stack data)
are skipped. Columns: track, query, a, b, c, px, py, pz (f32 hex bits),
out (space-separated).
"""
import argparse
from pathlib import Path
import random
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ai_harness import AiMachine, load_module, track_crp, ROOT  # noqa: E402
from vehicle_harness import SCRATCH  # noqa: E402

TRACKS = ['alps', 'autobahn', 'canyon', 'castle', 'coastal', 'farmland', 'foothills', 'forest',
          'industrial', 'monaco1', 'monaco2', 'monaco3', 'monaco4', 'monaco5', 'skidpad']
STEP, AHEAD, LOCATE, WALK, LOCATOR, PROGRESS = 0x4817d0, 0x4816f0, 0x481400, 0x4816c0, 0x474440, 0x481b70
POS = SCRATCH
LOC = SCRATCH + 0x20
OUT = SCRATCH + 0x60
PER_KIND = 300


def fb(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def record(game):
    module, data = load_module('nfs5.exe', game)
    rows = [['track', 'query', 'a', 'b', 'c', 'px', 'py', 'pz', 'out']]
    for t in TRACKS:
        m = AiMachine(module, data)
        crp = track_crp(game / f'GameData/Track/{t}.crp')
        jnc_path = game / f'GameData/Track/{t}.jnc'
        obj = m.build_track(crp, jnc_path.read_bytes() if jnc_path.exists() else None)
        g = m.dump_track(obj)
        n, nsec = g['count'], len(g['sections'])
        jun_begin = m.get_u32(m.get_u32(obj + 0x1c))
        nodes = m.get_u32(obj + 0xc)
        rnd = random.Random(f'ai-query-{t}')

        def node_pos(k):
            return m.get(nodes + k * 0x50 + 4, 3)

        def near_pos(k, spread):
            p = node_pos(k)
            return [p[0] + rnd.uniform(-spread, spread), p[1] + rnd.uniform(-3, 3),
                    p[2] + rnd.uniform(-spread, spread)]

        def emit(q, a, b, c, pos, out):
            pos = pos or [0.0, 0.0, 0.0]
            rows.append([t, q, str(a), str(b), str(c)] + [f'{fb(v):08x}' for v in pos]
                        + [' '.join(str(o) for o in out)])

        def call(entry, this, *args):
            try:
                return m.call_thiscall(entry, this, *args, budget=2_000_000)
            except RuntimeError:
                return None

        for _ in range(PER_KIND):
            node = rnd.randrange(n)
            delta = rnd.randint(-60, 60)
            branch = rnd.randint(-1, 2)
            r = call(STEP, obj, node, delta, branch)
            if r is not None:
                emit('step', node, delta, branch, None, [struct.unpack('<i', struct.pack('<I', r))[0]])
        for _ in range(PER_KIND):
            node, delta = rnd.randrange(n), rnd.randint(-60, 60)
            r = call(AHEAD, obj, node, delta)
            if r is not None:
                emit('ahead', node, delta, 0, None, [-1 if r == 0 else (r - jun_begin) // 0x38])
        for _ in range(PER_KIND):
            k = rnd.randrange(n)
            pos = near_pos(k, 25)
            hint = rnd.randrange(n) if rnd.random() < 0.3 else max(0, min(n - 1, k + rnd.randint(-40, 40)))
            m.put(POS, *pos)
            r = call(LOCATE, obj, POS, hint)
            if r is not None:
                emit('locate', hint, 0, 0, m.get(POS, 3), [r])
        for _ in range(PER_KIND):
            sec = rnd.randrange(nsec)
            k = rnd.randrange(n)
            pos = near_pos(k, 20)
            hint = rnd.randrange(n)
            m.put(POS, *pos)
            r = call(WALK, obj, sec, POS, hint)
            if r is not None:
                emit('walk', sec, hint, 0, m.get(POS, 3), [struct.unpack('<i', struct.pack('<I', r))[0]])
        for _ in range(PER_KIND):
            k = rnd.randrange(n)
            node = max(0, min(n - 1, k + rnd.randint(-8, 8)))
            sec = rnd.randint(-1, nsec - 1)
            pos = near_pos(k, 8)
            m.put(POS, *pos)
            m.uc.mem_write(LOC, struct.pack('<hhi', node, 0, sec) + bytes(0x20))
            r = call(LOCATOR, LOC, POS)
            if r is not None:
                b = m.get_bytes(LOC, 10)
                emit('locator', node, sec, 0, m.get(POS, 3),
                     [struct.unpack('<h', b[:2])[0], b[8], b[9]])
        for _ in range(PER_KIND // 3):
            node = rnd.randrange(n)
            r = call(PROGRESS, obj, node, OUT, OUT + 4, OUT + 8, OUT + 12)
            if r is not None:
                a, b = m.get_u32(OUT), m.get_u32(OUT + 4)
                c, d = m.get_i32(OUT + 8, 2)
                emit('progress', node, 0, 0, None, [f'{a:08x}', f'{b:08x}', c, d])
        print(t, file=sys.stderr)
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    p.add_argument('--out', type=Path, default=ROOT / 'local/fixtures/ai-track-query.tsv')
    p.add_argument('--check', action='store_true', help='record twice and compare')
    a = p.parse_args()
    rows = record(a.game_dir)
    if a.check and record(a.game_dir) != rows:
        sys.exit('two recordings differ')
    head = ['# Original track node queries (0x4817d0, 0x4816f0, 0x481400, 0x4816c0, 0x474440,',
            '# 0x481b70) on sampled inputs; recorded by scripts/research/replay-ai-query.py.']
    a.out.write_text('\n'.join(head + ['\t'.join(r) for r in rows]) + '\n')
    print(f'{len(rows) - 1} rows', file=sys.stderr)


if __name__ == '__main__':
    main()
