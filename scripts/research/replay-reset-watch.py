#!/usr/bin/env python3
"""Record the original reset watch `Car_ResetWatch` 0x40f740 on the closed-loop
forest bank runs (change original-wall-crash-response, task 2.2; workspace
docs/re/reset.md and docs/re/collision.md "Bank launch recording").

The harness is replay-bank-launch.py unchanged (real forest support polygons
with the EDG edges after them, node graph, on even steps `Car_TrackLocate`
0x49af70, the input word, `Car_ReadInputs`, `Car_DynamicsTick`,
`Car_GroundContactTick` 0x49bd40 with the tilted contact `0x496610` and the
overhead query `0x473330` unmodified, `Car_CollideTrackEdges` 0x495020), the
same 24 starts (nodes 70/160, steer pulse right/left, 100/125/150 km/h, 993 C4
and 356 A), but longer runs, and after the barrier pass of every even step the
watch `0x40f740` (prio 0x32, `Car_ContactEffectsTick` 0x410280 calls it for a
car without `+0x52c & 0x80`; its siblings `0x40a730` (car pairs) and
`0x40fff0` (scrape effects) are not run). The watch runs unmodified, and so
does a reset it starts: `Car_ResetToTrack` 0x40f540 with `Car_ResetNodeFree`
0x40f480 (racer and traffic counts 0), `TrackNodes_Step`,
`Car_ResetDriveState` 0x49e220, `Car_PlaceOnNode` 0x49b090 (with its ground
queries for an active car) and 0x40f400. Stubbed (effects only): the spark
burst 0x412230, the impact sound 0x43f4c0 (its `this`, `R3DCar+0x704`, is
read from a zeroed render object for the 993) and the camera snap 0x4618b0.
The effect event count `+0x110c` is put back after the watch (no consumer in
the harness; the bank rows keep only the ground tick's events). The race
direction `+0xeac` is 1 (forward race), as `0x403e40` sets it.

Per 32 Hz step the watch rows carry the fields the watch and `0x40f540` mode
0 read (before the call) and the timers after it, the `0x40f540` call (mode,
-1 none), whether it went past its early exits (`moved`, hook at the node
search 0x40f658) and the node handed to `0x49b090`. The body and dyn rows are
those of replay-bank-launch.py (the dyn row is taken after the barrier pass,
before the watch), so a reset shows in the next rows.

    python scripts/research/replay-reset-watch.py --game-dir local/game \\
        --polygons local/derived/support --out-dir local/fixtures
"""
import argparse
import importlib.util
import itertools
from pathlib import Path
import struct
import sys

from unicorn import UC_HOOK_CODE

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vehicle_harness as vh  # noqa: E402
import ai_harness as ah  # noqa: E402

_spec = importlib.util.spec_from_file_location('banklaunch', HERE / 'replay-bank-launch.py')
bl = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(bl)
cl, rs, vs = bl.cl, bl.rs, bl.vs
f32, word = vs.f32, vs.word
CAR = rs.CAR

RESET_WATCH = 0x40f740
RESET_TO_TRACK = 0x40f540
RESET_SEARCH = 0x40f658      # node search of 0x40f540 (past the early exits)
PLACE_ON_NODE = 0x49b090
STUBS = (0x412230, 0x4618b0)
SOUND = 0x43f4c0
# 0x40f540 / 0x40f480 globals: game mode (skidpad test), REVERSE, racer and
# traffic counts, screen count, player car id.
GLOBALS = {0x6573ec: 0, 0x657430: 0, 0x658080: 0, 0x65807c: 0, 0x5e99f4: 1, 0x65741c: 0}
STEPS = 1536                  # 24 s at 64 Hz
SPEEDS = bl.SPEEDS
CARS = bl.CARS
DRIVES = {'forest': [(n, s, p, STEPS) for n, s, p, _ in bl.BANKS['forest']]}

# Read by 0x40f740 / 0x40f540 mode 0 (column, offset, kind, count).
WATCH_IN = [
    ('flags_52c', 0x52c, 'i', 1), ('stuck_540', 0x540, 'i', 1), ('field_544', 0x544, 'i', 1),
    ('event_d8b', 0xd8b, 'b', 1), ('event_d89', 0xd89, 'b', 1), ('out_b50', 0xb50, 'i', 1),
    ('speed_35c', 0x35c, 'f', 1), ('impact_430', 0x430, 'f', 1), ('partner_434', 0x434, 'i', 1),
    ('up_3a0', 0x3a0, 'f', 1), ('start_db0', 0xdb0, 'i', 1), ('surface_464', 0x464, 'i', 1),
    ('node_up_3c4', 0x3c4, 'f', 3), ('car_up_370', 0x370, 'f', 3), ('ang_388', 0x388, 'f', 3),
    ('loc_b12', 0x12, 'b', 1), ('flags_418', 0x418, 'h', 1), ('height_424', 0x424, 'f', 1),
    ('node_08', 0x8, 'hs', 1), ('b7e', 0x7e, 'b', 1),
]
TIMERS = [('timer_1100', 0x1100, 'i', 1), ('cooldown_b4c', 0xb4c, 'i', 1),
          ('hit_1104', 0x1104, 'i', 1), ('field_1108', 0x1108, 'i', 1)]
FMT = {'f': ('<f', 4), 'i': ('<i', 4), 'h': ('<H', 2), 'hs': ('<h', 2), 'b': ('<B', 1)}


def read(m, fields, prefix=''):
    row = []
    for name, off, kind, n in fields:
        fmt, size = FMT[kind]
        vals = [struct.unpack(fmt, m.get_bytes(CAR + off + size * k, size))[0] for k in range(n)]
        row += [(f'{prefix}{name}', vals[0])] if n == 1 else \
            [(f'{prefix}{name}_{k}', v) for k, v in enumerate(vals)]
    return row


class ResetRecorder(bl.BankRecorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        m = self.m
        for va in STUBS:
            m.stubs[va] = lambda _m: 0
        # 0x43f4c0 is a thiscall with two arguments (`ret 8`).
        m.stubs[SOUND] = cl.vg.thiscall(2, lambda *_: 0)
        self.reset_mode = -1
        self.moved = 0
        self.place_node = -1

        def on_reset(uc, _address, _size, _user):
            sp = uc.reg_read(vh.UC_X86_REG_ESP)
            self.reset_mode = struct.unpack('<i', uc.mem_read(sp + 8, 4))[0]

        def on_search(_uc, _address, _size, _user):
            self.moved = 1

        def on_place(uc, _address, _size, _user):
            sp = uc.reg_read(vh.UC_X86_REG_ESP)
            self.place_node = struct.unpack('<i', uc.mem_read(sp + 8, 4))[0]

        m.uc.hook_add(UC_HOOK_CODE, on_reset, begin=RESET_TO_TRACK, end=RESET_TO_TRACK)
        m.uc.hook_add(UC_HOOK_CODE, on_search, begin=RESET_SEARCH, end=RESET_SEARCH)
        m.uc.hook_add(UC_HOOK_CODE, on_place, begin=PLACE_ON_NODE, end=PLACE_ON_NODE)

    def watch(self, run, n):
        m = self.m
        for addr, value in GLOBALS.items():
            m.put_u32(addr, value)
        before = read(m, WATCH_IN) + read(m, TIMERS, 'in_')
        events = m.get_u32(CAR + 0x110c)
        self.reset_mode, self.moved, self.place_node = -1, 0, -1
        m.call(RESET_WATCH, CAR, budget=50_000_000)
        m.put_u32(CAR + 0x110c, events)
        return ([('run', run), ('step', n)] + before + read(m, TIMERS, 'out_')
                + [('reset_mode', self.reset_mode), ('moved', self.moved),
                   ('place_node', self.place_node)]
                + [('out_pos_330_%d' % k, v) for k, v in enumerate(m.car_get(0x330, 3))]
                + [('out_axes_364_%d' % k, v) for k, v in enumerate(m.car_get(0x364, 9))]
                + [('out_start_db0', m.get_i32(CAR + 0xdb0)[0]),
                   ('out_node_08', struct.unpack('<h', m.get_bytes(CAR + 8, 2))[0])])

    def drive_reset(self, track, name, car, node, v0, sign, pulse, steps, body=None):
        m = self.m
        run = f'{track}/{name}'
        gear = self.setup_turned(car, node, 0.0, v0, body)
        # Race direction `+0xeac` = 1 (forward; `0x403e40` sets -1 with REVERSE): `0x49b090`
        # negates the node frame's lateral and forward when it is not 1.
        m.put_i32(CAR + 0xeac, 1)
        if m.get_u32(CAR + 0x74c) == 0:
            # Render object: 0x40f740 reads `R3DCar+0x704` (this of the stubbed sound 0x43f4c0).
            m.put_u32(CAR + 0x74c, self.alloc(0x800))
        bp = ','.join(f'{rs.bits(c):08x}' for p in self.body_points for c in p)
        init = [('run', run), ('track', track), ('car', car), ('node0', node), ('angle', f32(0.0)),
                ('v0', f32(v0)), ('gear0', gear), ('rpm0', self.f(0xdb4)), ('steps', steps),
                ('sched0', vs.SCHED0), ('rand_state0', m.get_u32(vs.G_RAND_STATE)), ('body_points', bp)]
        init += self.ground_row('init_') + self.read(CAR, cl.vg.INPUT_FIELDS, 'init_')
        body, dyn, watch = [], [], []
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
                watch.append(self.watch(run, n))
            m.call(vs.INTEGRATE_POS, CAR)
            m.call(vs.INTEGRATE_ORI, CAR)
            body.append(self.body_row(run, n))
        return init, body, dyn, watch


def record(game_dir, poly_dir):
    rec = ResetRecorder(game_dir)
    out = {'runs': [], 'body': [], 'dyn': [], 'watch': []}
    for track, drives in DRIVES.items():
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
            r, b, d, w = rec.drive_reset(track, name, car, node, v0, sign, pulse, steps, body)
            out['runs'].append(r)
            out['body'] += b
            out['dyn'] += d
            out['watch'] += w
            rows = [dict(x) for x in w]
            armed = next((x['step'] // 2 for x in rows if x['out_timer_1100'] > 0), None)
            calls = [(x['step'] // 2, x['reset_mode'], x['moved'], x['place_node'])
                     for x in rows if x['reset_mode'] >= 0]
            roof = next((x['step'] // 2 for x in rows
                         if sum(a * b for a, b in zip([x[f'node_up_3c4_{k}'] for k in range(3)],
                                                      [x[f'car_up_370_{k}'] for k in range(3)])) < 0.1), None)
            print(f'{track}/{name}: first dot<0.1 tick {roof}, armed tick {armed}, '
                  f'0x40f540 calls (tick, mode, moved, node) {calls}', flush=True)
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--polygons', type=Path, default=vh.ROOT / 'local/derived/support')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--once', action='store_true', help='skip the second (determinism) run')
    args = parser.parse_args()
    note = 'generated by scripts/research/replay-reset-watch.py (x87 control word 0x27f).'
    first = record(args.game_dir, args.polygons)
    if not args.once and repr(first) != repr(record(args.game_dir, args.polygons)):
        raise RuntimeError('two runs differ')
    o = args.out_dir
    vs.write_tsv(o / 'car-reset-runs.tsv', first['runs'], [
        'Forest bank runs of car-bank-* for 24 s with Car_ResetWatch 0x40f740 after the barrier pass; ' + note])
    vs.write_tsv(o / 'car-reset-body.tsv', first['body'], ['State after each 64 Hz step; ' + note])
    vs.write_tsv(o / 'car-reset-dyn.tsv', first['dyn'], [
        'Dyn, ground and locator fields after the barrier pass of each 32 Hz step (as car-bank-dyn); ' + note])
    vs.write_tsv(o / 'car-reset-watch.tsv', first['watch'], [
        'Per 32 Hz step: fields read by 0x40f740 (before), timers after, the 0x40f540 call '
        '(reset_mode -1 none; moved = past the early exits; place_node = 0x49b090 node) and the pose after; ' + note])
    print(f"{len(first['runs'])} runs, {len(first['body'])} steps -> {o}")


if __name__ == '__main__':
    main()
