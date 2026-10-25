#!/usr/bin/env python3
"""Record the original track node graph of every track (change original-ai,
task 2.1; docs/re/ai.md section 2 in the workspace root).

Runs the original builder (0x47fd30, TrackNodes_Build 0x480140 with the
track CRP's SimD/SimT chunks) and the .jnc loader 0x481080 in Unicorn
(ai_harness.py) and dumps the resulting tables. Floats are written as their
32-bit patterns in hex.

Output (one block per track, tab-separated):
  T  track  node_count  lap_nodes(+0x4)  lap_end(+0x8)
  S  index  (node section weight end) x2  junction_start  junction_end
  J  index  field_24  progress_28  centre_x  centre_y  centre_z  ends...
     (each end `section.end`)
  JS index  all 8 end slots (stale entries past the count; - null, x not
     a section end: stack data of the builder)
  B10  hex bytes of +0x10 (nearest junction per node, ff = none)
  B14  hex bytes of +0x14 (junction at a section end, ff = none)
  N  junction  node:dist2 ...   (near-node list +0x20)
"""
import argparse
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ai_harness import AiMachine, load_module, track_crp, ROOT  # noqa: E402

TRACKS = ['alps', 'autobahn', 'canyon', 'castle', 'coastal', 'farmland', 'foothills', 'forest',
          'industrial', 'monaco1', 'monaco2', 'monaco3', 'monaco4', 'monaco5', 'skidpad']


def hx(bits):
    return f'{bits & 0xffffffff:08x}'


def fbits(x):
    return hx(struct.unpack('<I', struct.pack('<f', x))[0])


def record(game, tracks):
    module, data = load_module('nfs5.exe', game)
    lines = []
    for t in tracks:
        m = AiMachine(module, data)
        crp = track_crp(game / f'GameData/Track/{t}.crp')
        jnc_path = game / f'GameData/Track/{t}.jnc'
        jnc = jnc_path.read_bytes() if jnc_path.exists() else None
        obj = m.build_track(crp, jnc)
        d = m.dump_track(obj)
        lines.append(f"T\t{t}\t{d['count']}\t{d['lap_nodes']}\t{d['lap_end']}")
        for i, (ends, js, je) in enumerate(d['sections']):
            cols = []
            for node, sec, wbits, end in ends:
                cols += [str(node), str(sec), hx(wbits), str(end)]
            lines.append('\t'.join(['S', str(i)] + cols + [str(js), str(je)]))
        for j, (eps, (f24, f28, cx, cy, cz), slots) in enumerate(d['junctions']):
            ends = [f'{s}.{e}' for s, e in eps]
            lines.append('\t'.join(['J', str(j), str(f24), hx(f28), fbits(cx), fbits(cy), fbits(cz)]
                                   + ends))
        for j, (_, _, slots) in enumerate(d['junctions']):
            cols = ['-' if v is None else v if v == 'x' else f'{v[0]}.{v[1]}' for v in slots]
            lines.append('\t'.join(['JS', str(j)] + cols))
        lines.append('B10\t' + d['tab10'].hex())
        lines.append('B14\t' + d['tab14'].hex())
        for j, near in enumerate(d['near']):
            lines.append('\t'.join(['N', str(j)] + [f'{n}:{hx(b)}' for n, b in near]))
        print(f"{t}: {d['count']} nodes, {len(d['sections'])} sections, "
              f"{len(d['junctions'])} junctions", file=sys.stderr)
    return lines


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    p.add_argument('--out', type=Path, default=ROOT / 'local/fixtures/ai-track-graph.tsv')
    p.add_argument('--tracks', nargs='*', default=TRACKS)
    p.add_argument('--check', action='store_true', help='record twice and compare')
    a = p.parse_args()
    lines = record(a.game_dir, a.tracks)
    if a.check and record(a.game_dir, a.tracks) != lines:
        sys.exit('two recordings differ')
    head = ['# Original track node graphs (TrackNodes_Build 0x480140, .jnc 0x481080),',
            '# recorded by scripts/research/replay-ai-track.py; floats as hex bit patterns.']
    a.out.write_text('\n'.join(head + lines) + '\n')


if __name__ == '__main__':
    main()
