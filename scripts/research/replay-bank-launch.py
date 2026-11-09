#!/usr/bin/env python3
"""Record the original car driven off the road onto the Schwarzwald (`forest`)
roadside banks at 100, 125 and 150 km/h (change original-wall-crash-response,
task 2; docs/re/collision.md "Bank launch recording" in the workspace root).

The barrier set of the car launch recorder (replay-car-launch.py): the real
forest support polygons (polygon-only tree plus the track's EDG edges), the
node graph, and on even steps `Car_TrackLocate` 0x49af70, the input word,
`Car_ReadInputs`, `Car_DynamicsTick`, `Car_GroundContactTick` 0x49bd40 (with
the tilted-car contact `0x496610` unmodified, its normal from the body owner
car+8 by `0x474420`) and `Car_CollideTrackEdges` 0x495020; `0x49b7a0`,
`0x49b8e0` on every step. The overhead query 0x473330 returns none (A28).

The drive is the engine's headless one: the 993 C4 (V6 body points) and the
356 A (its CRP `BndP` points, as replay-car-launch.py) on the node centre
(heading the node forward, 5 cm above the support), `v0` along it, the
keyboard words of "hold W, steer pulse": gas 126 (`(0xff/2)` clamped, the
keyboard word) on every poll, steer +-63 (`(R-L)/4`) for the first
`pulse` 32 Hz polls, then 0.

Per 32 Hz step the dyn rows carry, besides the car-launch columns, the
ground branch the tick ran (code hooks in 0x49bd40): 3 = tilted contact
(call 0x496610 at 0x49c000), 2 = hard landing (0x49c1c4, `closing > 6`,
`h < -0.05`; `0x493dd0` at 0x49c1ce for racers), 1 = soft contact
(0x49c28e), 0 = none of them (airborne or above 0.005 m). The body owner
normal (`0x474420` on car+8 = car+0x14) is `loc_n_*`, the alternate plane
`alt_n_3e8_*`, `alt_b_400_*`, the body surface `body_flags_2c`.

    python scripts/research/replay-bank-launch.py --game-dir local/game \\
        --polygons local/derived/support --out-dir local/fixtures
"""
import argparse
import itertools
import importlib.util
from pathlib import Path
import sys

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EAX

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vehicle_harness as vh  # noqa: E402
import ai_harness as ah  # noqa: E402

_spec = importlib.util.spec_from_file_location('carlaunch', HERE / 'replay-car-launch.py')
cl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cl)
rs, vs = cl.rs, cl.vs
f32, word = vs.f32, vs.word
CAR = rs.CAR

ROOF = True
ROOF_RESULT = 0x49c110   # `test ax, ax` after `call 0x473330` in 0x49bd40
BRANCHES = {0x49c000: 3, 0x49c1c4: 2, 0x49c28e: 1}
SPEEDS = (('100', 27.8), ('125', 34.7), ('150', 41.7))
# (node, steer sign, pulse in 32 Hz polls, 64 Hz steps)
# The 993 C4 with the V6 body points (as the other road recorders) and the user's 356 A with
# its CRP `BndP` points (replay-car-launch.py).
CARS = ('993C4', '356A')
BANKS = {'forest': [(160, 1, 22, 384), (160, -1, 22, 384), (70, 1, 22, 384), (70, -1, 22, 384)]}


class BankRecorder(cl.LaunchRecorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        self.branch = 0

        def hit(_uc, address, _size, _user):
            self.branch = max(self.branch, BRANCHES[address])

        for va in BRANCHES:
            self.m.uc.hook_add(UC_HOOK_CODE, hit, begin=va, end=va)
        self.roof = 0
        if ROOF:
            # The overhead query runs unmodified on the body owner car+8.
            self.m.stubs.pop(cl.vg.TRACK_QUERY, None)

        def roof(uc, _address, _size, _user):
            self.roof = uc.reg_read(UC_X86_REG_EAX) & 0xffff

        self.m.uc.hook_add(UC_HOOK_CODE, roof, begin=ROOF_RESULT, end=ROOF_RESULT)

    def drive_bank(self, track, name, car, node, v0, sign, pulse, steps, body=None):
        m = self.m
        run = f'{track}/{name}'
        gear = self.setup_turned(car, node, 0.0, v0, body)
        bp = ','.join(f'{rs.bits(c):08x}' for p in self.body_points for c in p)
        init = [('run', run), ('track', track), ('car', car), ('node0', node), ('angle', f32(0.0)),
                ('v0', f32(v0)), ('gear0', gear), ('rpm0', self.f(0xdb4)), ('steps', steps),
                ('sched0', vs.SCHED0), ('rand_state0', m.get_u32(vs.G_RAND_STATE)), ('body_points', bp)]
        init += self.ground_row('init_') + self.read(CAR, cl.vg.INPUT_FIELDS, 'init_')
        body, dyn = [], []
        for n in range(steps):
            s = vs.SCHED0 + n
            m.put_u32(vs.G_SCHED, s)
            if s % 2 == 0:
                m.call(rs.TRACK_LOCATE, CAR)
                k = len(dyn)
                w = word(gas=252, steer=126 * sign if k < pulse else 0)
                m.put_u32(vs.G_INPUT_WORD, w)
                m.call(vs.READ_INPUTS, CAR)
                m.call(vs.DYNAMICS_TICK, CAR)
                m.put_u32(CAR + 0x438, 0)
                self.branch = 0
                self.roof = 0
                if not self.tick():
                    raise RuntimeError(f'{run} step {n}: reached {sorted(map(hex, self.reached))}')
                branch, roof = self.branch, self.roof
                contact = m.get_u32(CAR + 0x438)
                m.put_u32(CAR + 0x438, 0)
                m.call(cl.BARRIER_PASS, CAR, budget=20_000_000)
                contact = contact | m.get_u32(CAR + 0x438) << 32
                dyn.append(self.dyn_row(run, n, w) + self.ground_row(full=False) + self.locate_row()
                           + [('contact_438', contact), ('impact_430', m.car_get(0x430)[0]),
                              ('branch', branch), ('roof_473330', roof)])
            m.call(vs.INTEGRATE_POS, CAR)
            m.call(vs.INTEGRATE_ORI, CAR)
            body.append(self.body_row(run, n))
        return init, body, dyn


def record(game_dir, poly_dir):
    rec = BankRecorder(game_dir)
    out = {'runs': [], 'body': [], 'dyn': []}
    for track, drives in BANKS.items():
        polygons = rs.load_polygons(poly_dir / f'{track}-polygons.tsv')
        crp = ah.track_crp(game_dir / 'GameData/Track' / f'{track}.crp')
        rec.build_support(polygons)
        rec.build_edges(game_dir, track)
        rec.build_nodes(crp)
        for car, (node, sign, pulse, steps), (kmh, v0) in itertools.product(CARS, drives, SPEEDS):
            body = None
            if car in cl.BNDP_MODEL:
                model, half = cl.BNDP_MODEL[car]
                body = (cl.bndp_points(game_dir, model), half)
            name = f'n{node}_{"r" if sign > 0 else "l"}_{kmh}' + ('' if car == '993C4' else f'_{car.lower()}')
            r, b, d = rec.drive_bank(track, name, car, node, v0, sign, pulse, steps, body)
            out['runs'].append(r)
            out['body'] += b
            out['dyn'] += d
            rows = [dict(x) for x in d]
            peak = max(x['height_424'] for x in rows)
            vy = max(x['vel_33c_1'] for x in rows)
            br = [sum(1 for x in rows if x['branch'] == k) for k in range(4)]
            roofs = [i for i, x in enumerate(rows) if x['roof_473330']]
            print(f'{track}/{name}: peak +0x424 {peak:.2f} m, max vy {vy:.2f}, '
                  f'branches none/soft/hard/tilt {br}, 0x473330 hits {len(roofs)} {roofs[:8]}', flush=True)
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--polygons', type=Path, default=vh.ROOT / 'local/derived/support')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--once', action='store_true', help='skip the second (determinism) run')
    args = parser.parse_args()
    note = 'generated by scripts/research/replay-bank-launch.py (x87 control word 0x27f).'
    first = record(args.game_dir, args.polygons)
    if not args.once and repr(first) != repr(record(args.game_dir, args.polygons)):
        raise RuntimeError('two runs differ')
    o = args.out_dir
    vs.write_tsv(o / 'car-bank-runs.tsv', first['runs'], [
        'Forest nodes 70/160, hold gas, steer pulse onto the roadside bank, with EDG edges; ' + note])
    vs.write_tsv(o / 'car-bank-body.tsv', first['body'], ['State after each 64 Hz step; ' + note])
    vs.write_tsv(o / 'car-bank-dyn.tsv', first['dyn'], [
        'Dyn, ground and locator fields after each 32 Hz step; contact_438 as car-barrier; '
        'branch: 0 none, 1 soft, 2 hard landing, 3 tilted contact 0x496610; ' + note])
    print(f"{len(first['runs'])} runs, {len(first['body'])} steps -> {o}")


if __name__ == '__main__':
    main()
