#!/usr/bin/env python3
"""Record the original car driven into the road-border hills of Auvergne
(change fix-car-launch; docs/re/support.md and vehicle.md in the workspace
root).

The road support recorder (replay-road-support.py) on the castle track, with
two differences:
- the car starts on a node near the start, turned by `angle` degrees about
  the node up from the node forward, at `v0` along that heading, and drives
  with the steering word 0 (gas up to `vmax` on d60), so it runs into the
  verge hill at the road border;
- the tilted-car contact `0x496610` (with `0x495f30`, `0x4964d0`,
  `Car_ImpactByDirection` 0x494df0) runs unmodified instead of stopping the
  run: it is what the original does on the hill. `+0x438` is recorded so the
  steps that reached it are visible (`0x495f30` writes `+0x464 | 0x30000`).

`car-launch-*`: no barrier pass (polygon-only tree, as the road support
recorder), so the car may run through the barrier edge at the top of the
hill. `car-barrier-*`: the same with the track's EDG edges constructed by
`0x485100` from `<track>.edg` (records of 28 bytes, word = byte 0, as
`0x487e80` reads them) and inserted after the polygons (the retained order of
`0x488090`), and the barrier pass `Car_CollideTrackEdges` 0x495020 run after
the ground tick on even steps (`Car_CollideTick` 0x498ff0 calls it first for
the player; no track objects or other cars here). It runs unmodified, with
`0x494000`, `0x493f10`, `0x493e40`, `Car_ImpactByDirection` and
`Car_ApplyImpact` (mode 1). The damage gate `[0x657a2c]` is 0.

    python scripts/research/replay-car-launch.py --game-dir local/game \\
        --polygons local/derived/support --out-dir local/fixtures
"""
import argparse
import importlib.util
import math
from pathlib import Path
import struct
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vehicle_harness as vh  # noqa: E402
import ai_harness as ah  # noqa: E402

_spec = importlib.util.spec_from_file_location('rsupport', HERE / 'replay-road-support.py')
rs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(rs)
vg, vs = rs.vg, rs.vs
f32, word = vs.f32, vs.word
CAR = rs.CAR

# (name, start (x, z) original axes, heading offset in degrees, v0, vmax, 64 Hz steps)
DRIVES = {
    'castle': [('hill_left', (83.7, 783.8), 45.0, 13.9, 13.9, 192),
               ('hill_right', (83.7, 783.8), -45.0, 13.9, 13.9, 192),
               ('hill_left_fast', (83.7, 783.8), 45.0, 22.0, 22.0, 160),
               ('hill30_fast', (83.7, 783.8), 30.0, 22.0, 22.0, 160),
               ('hill60_fast', (83.7, 783.8), 60.0, 22.0, 22.0, 160),
               ('hill_right_fast', (83.7, 783.8), -45.0, 22.0, 22.0, 160),
               # Launches found by sweeping the heading in the port.
               ('launch_m30_100kmh', (83.7, 783.8), -30.0, 28.0, 28.0, 160),
               ('launch_m35_80kmh', (83.7, 783.8), -35.0, 22.0, 22.0, 192)],
}


# Barrier runs: (name, start (x, z), heading offset, v0, vmax, steps), castle and alps.
BARRIER_DRIVES = {
    'castle': [('hill_left', (83.7, 783.8), 45.0, 13.9, 13.9, 256),
               ('hill_right', (83.7, 783.8), -45.0, 13.9, 13.9, 256),
               ('hill30', (83.7, 783.8), 30.0, 13.9, 13.9, 256),
               ('hill_m30_fast', (83.7, 783.8), -30.0, 22.0, 22.0, 224),
               ('hill_left_fast', (83.7, 783.8), 45.0, 22.0, 22.0, 224)],
    # Hard side hits into the Alps start barrier at 144 km/h (impact > 80).
    'alps': [('wall_60', (-132.1, 488.0), 60.0, 40.0, 40.0, 96),
             ('wall_90', (-132.1, 488.0), 90.0, 40.0, 40.0, 96),
             ('wall_m75', (-132.1, 488.0), -75.0, 40.0, 40.0, 96)],
}
# The user's car: 356 A (`356Acoupe16.sim`) with its own body points, the CRP `BndP` points as
# `Car_ReadTpgCameraKeys` 0x441a10 stores them in the render object (`R3DCar+0xf8` count,
# `+0xfc` points, y minus the half height `+0x3ac`) and `Car_SetupBodyPoints` 0x49abc0
# orders them (run unmodified). Half extents: the level-5 box (`collision.md`).
BARRIER_DRIVES['castle'] += [
    ('hill_left_356a', (83.7, 783.8), 45.0, 13.9, 13.9, 320, '356A'),
    ('hill_left_356a_fast', (83.7, 783.8), 45.0, 19.0, 19.0, 320, '356A'),
    ('hill_right_356a_fast', (83.7, 783.8), -45.0, 19.0, 19.0, 320, '356A'),
]
BNDP_MODEL = {'356A': ('356a', (0.84328, 0.60407, 1.88003))}
SETUP_BODY_POINTS = 0x49abc0
EDGE_NEW, EDGE_SIZE = 0x485100, 0x2c
BARRIER_PASS = 0x495020


def bndp_points(game_dir, model):
    """`BndP` entries of a car CRP with part 0 or value 0 (raw model coordinates)."""
    data = (game_dir / 'GameData/CarModel' / f'{model}.crp').read_bytes()
    if data[:4] != b' raC':
        data = ah.refpack(data)
    out, i = [], 0
    while True:
        i = data.find(b'PdnB', i)
        if i < 0:
            return out
        _tag, flags, count, off = struct.unpack_from('<4sIII', data, i)
        p = (i if flags & 2 else 0) + off
        part, value = struct.unpack_from('<ii', data, p)
        if part == 0 or value == 0:
            out += [struct.unpack_from('<3f', data, p + 8 + 16 * k) for k in range(count)]
        i += 4


class LaunchRecorder(rs.SupportRecorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        # The original tilted-car contact runs (no stub).
        for va in (vg.ROLLOVER,):
            self.m.stubs.pop(va, None)

    def build_edges(self, game_dir, track):
        """EDG objects as 0x487e80 builds them, inserted after the polygons."""
        m = self.m
        raw = (game_dir / 'GameData/Track' / f'{track}.edg').read_bytes()
        n = (len(raw) - 4) // 28
        tree = m.get_u32(m.get_u32(rs.SCENE_PTR) + 0x24)
        root = m.get_u32(tree)
        src = self.alloc(0x20)
        for k in range(n):
            o = 4 + 28 * k
            word = raw[o]
            p = struct.unpack_from('<6f', raw, o + 4)
            m.put(src, p[0], p[1], p[2], 0.0, p[3], p[4], p[5], 0.0)
            obj = self.alloc(EDGE_SIZE)
            m.call_thiscall(EDGE_NEW, obj, src, word)
            m.call_thiscall(rs.TREE_INSERT, root, obj, budget=50_000_000)
        self.edge_count = n

    def setup_body_points(self, points, half):
        """R3DCar point list as 0x441a10 leaves it, then 0x49abc0 on the car."""
        m = self.m
        r3d = self.alloc(0xfc + 12 * len(points))
        m.put_i32(r3d + 0xf8, len(points))
        for k, p in enumerate(points):
            m.put(r3d + 0xfc + 12 * k, p[0], f32(p[1] - f32(half[1])), p[2])
        m.put_u32(CAR + 0x74c, r3d)
        m.car_put(0x3a8, *[f32(h) for h in half])
        m.call(SETUP_BODY_POINTS, CAR)
        n = m.get_i32(CAR + 0x32c)[0]
        return [tuple(m.car_get(0x8c + 12 * k, 3)) for k in range(n)]

    def setup_turned(self, car, node, angle, v0, body=None):
        gear = self.setup_drive(car, node, 1, v0)
        m = self.m
        points = vg.BODY_POINTS
        if body is not None:
            points = self.setup_body_points(*body)
        # Mass and inertia as `0x410730`/`Car_ResetBody` 0x49b510 set them (read by 0x495f30):
        # m = sim+0x40 / 128, I = 6m.
        mass = f32(self.fields[car]['mass_40'] / 128.0)
        m.car_put(0x354, mass, f32(1.0 / mass))
        m.car_put(0x394, f32(6.0 * mass), f32(1.0 / (mass * 6.0)))
        r, u, f = (m.car_get(0x364, 3), m.car_get(0x370, 3), m.car_get(0x37c, 3))
        a = math.radians(angle)
        ca, sa = math.cos(a), math.sin(a)
        f2 = tuple(f32(ca * f[i] + sa * r[i]) for i in range(3))
        r2 = tuple(f32(ca * r[i] - sa * f[i]) for i in range(3))
        m.car_put(0x364, *r2, *u, *f2)
        pos = m.car_get(0x330, 3)
        vel = tuple(f32(v0 * x) for x in f2)
        m.car_put(0x33c, *vel)
        m.car_put(0x35c, vs.octagonal(vel[0], vel[2]))
        rows = list(zip(r2, u, f2))
        for k, pt in enumerate(points[:4]):
            w = [f32(sum(pt[j] * rows[i][j] for j in range(3)) + pos[i]) for i in range(3)]
            m.put(m.wheel(k) + 0x40, *w)
        m.call(0x49ae30, CAR)
        self.body_points = points
        up = m.car_get(0x3a0)[0]
        m.call(vs.GROUND_FRAME, CAR, vs.NORMAL, rs.bits(up))
        return gear

    def drive_turned(self, track, name, car, node, angle, v0, vmax, steps, barrier=False, body=None):
        m = self.m
        run = f'{track}/{name}'
        gear = self.setup_turned(car, node, angle, v0, body)
        # Body points `+0x8c` as f32 bits (x, y, z per point), in list order.
        bp = ','.join(f'{rs.bits(c):08x}' for p in self.body_points for c in p)
        init = [('run', run), ('track', track), ('car', car), ('node0', node), ('angle', f32(angle)),
                ('v0', f32(v0)), ('gear0', gear), ('rpm0', self.f(0xdb4)), ('steps', steps),
                ('sched0', vs.SCHED0), ('rand_state0', m.get_u32(vs.G_RAND_STATE)), ('body_points', bp)]
        init += self.ground_row('init_') + self.read(CAR, vg.INPUT_FIELDS, 'init_')
        body, dyn = [], []
        for n in range(steps):
            s = vs.SCHED0 + n
            m.put_u32(vs.G_SCHED, s)
            if s % 2 == 0:
                m.call(rs.TRACK_LOCATE, CAR)
                gas = 254 if m.car_get(0xd60)[0] < vmax else 0
                w = word(gas=gas, steer=0)
                m.put_u32(vs.G_INPUT_WORD, w)
                m.call(vs.READ_INPUTS, CAR)
                m.call(vs.DYNAMICS_TICK, CAR)
                m.put_u32(CAR + 0x438, 0)
                if not self.tick():
                    raise RuntimeError(f'{run} step {n}: reached {sorted(map(hex, self.reached))}')
                contact = m.get_u32(CAR + 0x438)
                if barrier:
                    m.put_u32(CAR + 0x438, 0)
                    m.call(BARRIER_PASS, CAR, budget=20_000_000)
                    contact = contact | m.get_u32(CAR + 0x438) << 32
                dyn.append(self.dyn_row(run, n, w) + self.ground_row(full=False) + self.locate_row()
                           + [('contact_438', contact), ('impact_430', m.car_get(0x430)[0])])
            m.call(vs.INTEGRATE_POS, CAR)
            m.call(vs.INTEGRATE_ORI, CAR)
            body.append(self.body_row(run, n))
        return init, body, dyn


def record(game_dir, poly_dir, barrier=False):
    rec = LaunchRecorder(game_dir)
    out = {'runs': [], 'body': [], 'dyn': []}
    for track, drives in (BARRIER_DRIVES if barrier else DRIVES).items():
        polygons = rs.load_polygons(poly_dir / f'{track}-polygons.tsv')
        crp = ah.track_crp(game_dir / 'GameData/Track' / f'{track}.crp')
        rec.build_support(polygons)
        if barrier:
            rec.build_edges(game_dir, track)
        rec.build_nodes(crp)
        for name, xz, angle, v0, vmax, steps, *car in drives:
            node = rs.nearest_node(rec, *xz)
            car = car[0] if car else '993C4'
            body = None
            if car in BNDP_MODEL:
                model, half = BNDP_MODEL[car]
                body = (bndp_points(game_dir, model), half)
            r, b, d = rec.drive_turned(track, name, car, node, angle, v0, vmax, steps, barrier, body)
            out['runs'].append(r)
            out['body'] += b
            out['dyn'] += d
            hits = sum(1 for row in d if (dict(row)['contact_438'] & 0xffffffff) >> 16 == 3)
            walls = sum(1 for row in d if dict(row)['contact_438'] >> 48 == 4)
            vy = max(dict(row)['vel_33c_1'] for row in d)
            print(f'{track}/{name}: node {node}, {steps} steps, 0x496610 contact on {hits} '
                  f'32 Hz steps, barrier on {walls}, max vy {vy:.2f}', flush=True)
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--polygons', type=Path, default=vh.ROOT / 'local/derived/support')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--once', action='store_true', help='skip the second (determinism) run')
    parser.add_argument('--only', choices=('launch', 'barrier'), help='record one set only')
    args = parser.parse_args()
    note = 'generated by scripts/research/replay-car-launch.py (x87 control word 0x27f).'
    o = args.out_dir
    for kind in ('launch', 'barrier'):
        if args.only and args.only != kind:
            continue
        first = record(args.game_dir, args.polygons, kind == 'barrier')
        if not args.once:
            # repr: rows may hold NaN (the original asin of a heading slightly over 1).
            if repr(first) != repr(record(args.game_dir, args.polygons, kind == "barrier")):
                raise RuntimeError('two runs differ')
        what = 'without' if kind == 'launch' else 'with the barrier pass 0x495020 and'
        vs.write_tsv(o / f'car-{kind}-runs.tsv', first['runs'], [
            f'Castle start, car turned into the border hill, steer 0, {what} EDG edges; ' + note])
        vs.write_tsv(o / f'car-{kind}-body.tsv', first['body'], ['State after each 64 Hz step; ' + note])
        vs.write_tsv(o / f'car-{kind}-dyn.tsv', first['dyn'], [
            'Dyn, ground and locator fields after each 32 Hz step; contact_438: +0x438 after the '
            'ground tick, | after the barrier pass << 32; ' + note])
        print(f"{kind}: {len(first['runs'])} runs, {len(first['body'])} steps -> {o}")


if __name__ == '__main__':
    main()
