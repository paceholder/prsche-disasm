#!/usr/bin/env python3
"""Record closed-loop AI runs (change original-ai, task 5.1; docs/re/ai.md in
the workspace root): the original AI drives the original car model.

Machine: ai_harness.AiMachine (track graph by the original builder, racing
line, CRT acos error report skipped). Car: carSpecs from the V1 derivation,
AISpecs from the original Ais_Load 0x49ca30, `+0xda8 = 1` (AI), racer flag
`+0x52c = 1`, alone in the race order (`[0x5e8b88] = car`, count 1, player
`[0x5e8b34] = car`), SKILL 0, mode 0, race running `[0x606ac0] = 1`.

Each 64 Hz step s (as replay-vehicle-step.py, the V5 recorder):
- even s: the track locator update `TrackLocator_Update` 0x474440 (thiscall
  on car+8, the prio-6 part of Car_TrackLocate that the AI reads) and, when the
  node changed (car+0x11), the node frame copy of 0x499520 (+0x3b8 lateral,
  +0x3c4 up, +0x3d0 forward; done here); then Car_ReadInputs 0x4991e0 (AI
  branch: AI_TargetSpeed 0x40b650, AI_Drive 0x40bfe0) with input word 0,
  Car_DynamicsTick 0x49fd30, and the flat-ground stand-in: ground frame
  0x49ba30 with normal (0,1,0), +0x35c octagonal speed, and the height
  stand-in pos.y = node.y + 0.5 (wheel points w+0x40 moved with it);
- every s: Car_IntegratePosition 0x49b7a0, Car_IntegrateOrientation 0x49b8e0.

Columns per even step (floats as f32 hex bits): run, step, node_08,
section_0c, junction_e94, branch_e98, class_e9c, skill_ea0, target_eb8,
stuck_ea4, event_d8b, gas_d9c, brake_d9d, steer_da0, gear_d82, shift_d84,
gas_d7f, brake_d80, steer_d94, rpm_db4, fwd_d60, pos_330 (3), vel_33c (3),
ang_388 (3), f_37c (3), rand_state.
"""
import argparse
import math
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vehicle_harness as vh  # noqa: E402
from ai_harness import AiMachine, track_crp  # noqa: E402

READ_INPUTS, DYNAMICS_TICK = 0x4991e0, 0x49fd30
GROUND_FRAME, INTEGRATE_POS, INTEGRATE_ORI = 0x49ba30, 0x49b7a0, 0x49b8e0
LOCATOR, AIS_LOAD, FILE_OPEN = 0x474440, 0x49ca30, 0x59d9b0
G_EFFECTS_ON = 0x6197e7
G_INPUT_WORD, G_NET_PLAYERS, G_MIRROR = 0x5e9664, 0x5e8e28, 0x65742c
G_THROTTLE, G_BRAKE, G_STEER_ABS = 0x628fb0, 0x628fac, 0x628fb4
G_SCHED, G_RAND_STATE, G_RAND_MUL, G_RAND_LAST = 0x606ac4, 0x5d1028, 0x5d102c, 0x655a0c
CALLBACK_STUB = vh.EXIT + 0x100
NORMAL = vh.SCRATCH
AIS = vh.SPECS + 0x400
WHEEL_POINTS = [(-0.75, 0.0, 1.2), (0.75, 0.0, 1.2), (-0.75, 0.0, -1.2), (0.75, 0.0, -1.2)]
BODY_POINTS = [(-0.9, 0.5, 2.1), (0.9, 0.5, 2.1), (-0.9, 0.5, -2.1), (0.9, 0.5, -2.1)]
G = {'skill': 0x6573f4, 'reverse': 0x657430, 'mode': 0x6573e8, 'running': 0x606ac0,
     'count': 0x5e8b58, 'player': 0x5e8b34, 'track': 0x657444, 'order': 0x5e8b88}

# (run, track, reverse, sim, model, start node, speed m/s, gear (1 = N), sched0, db0, steps)
RUNS = [
    ('alps_start_356A', 'alps', 0, '356Acoupe16', '356A', 30, 0.0, 1, 0x1a0, 0x20, 1600),
    ('alps_rolling_356A', 'alps', 0, '356Acoupe16', '356A', 400, 25.0, 3, 0x1000, 0x100, 1200),
    ('monaco1_start_boxster', 'monaco1', 0, 'boxster25', 'boxster', 10, 0.0, 1, 0x1a0, 0x20, 1600),
    ('alps_rev_993', 'alps', 1, '993coupe436', '993', 1700, 20.0, 3, 0x1000, 0x100, 1200),
]


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def fb(x):
    return f'{struct.unpack("<I", struct.pack("<f", x))[0]:08x}'


def octagonal(vx, vz):
    a, b = abs(vx), abs(vz)
    return f32(b * 0.25 + a) if a > b else f32(a * 0.25 + b)


class Loop:
    def __init__(self, game):
        self.game = game
        self.module, self.data = vh.load_module('nfs5.exe', game)

    def run(self, name, track, reverse, sim, model, node0, v0, gear0, sched0, db0, steps):
        game = self.game
        m = AiMachine(self.module, self.data, stubs={FILE_OPEN: lambda mm: mm.file_addr,
                                                      CALLBACK_STUB: lambda mm: 0})
        crp = track_crp(game / f'GameData/Track/{track}.crp')
        jnc = game / f'GameData/Track/{track}.jnc'
        obj = m.build_track(crp, jnc.read_bytes() if jnc.exists() else None)
        m.set_racing_line((game / f'GameData/Track/racer/work/{track}{reverse}.lsp').read_bytes())
        jun_begin = m.get_u32(m.get_u32(obj + 0x1c))
        nodes = m.get_u32(obj + 0xc)
        raw = (game / f'GameData/Simulation/CarData/{sim}.sim').read_bytes()
        vh.derive_specs(m, raw, model)          # resets the car, writes res and specs
        m.put_str(G['track'], track, 0x20)
        m.put_u32(G['reverse'], reverse)
        m.put_u32(vh.CAR + 0x764, AIS)
        m.file_addr = m.place_file((game / f'GameData/Simulation/AICarData/{model}.ais').read_bytes())
        m.call(AIS_LOAD, vh.CAR, budget=5_000_000)
        m.set_globals(g_SchedStep=sched0)
        for key, val in (('skill', 0), ('mode', 0), ('running', 1), ('count', 1),
                         ('player', vh.CAR), ('order', vh.CAR)):
            m.put_u32(G[key], val)
        m.put_i32(vh.RES + 0x14, 1)
        for off, val in ((0x154, 1.0), (0x158, 1.0), (0x15c, 1.0), (0x160, 1.0)):
            m.put(vh.RES + off, val)
        m.put_i32(G_NET_PLAYERS, 0)
        m.put_i32(G_MIRROR, 0)
        m.put_bytes(G_EFFECTS_ON, [0])
        for addr in (G_THROTTLE, G_BRAKE, G_STEER_ABS, G_RAND_LAST, G_INPUT_WORD):
            m.put_u32(addr, 0)
        m.put_u32(vh.CAR + 0xe10, CALLBACK_STUB)
        m.put_i32(vh.CAR + 0xda8, 1)
        m.put_i32(vh.CAR + 0x52c, 1)
        m.put_bytes(vh.CAR + 0x7f, [1])
        m.put_bytes(vh.CAR + 0x80, [0x20])
        m.put(vh.CAR + 0xea0, 1.0)
        rec = m.get(nodes + node0 * 0x50 + 4, 12)
        npos, nup, nfwd, nlat = rec[0:3], rec[3:6], rec[6:9], rec[9:12]
        sgn = -1.0 if reverse else 1.0
        fwd = [f32(sgn * nfwd[0]), 0.0, f32(sgn * nfwd[2])]
        n = math.sqrt(fwd[0] ** 2 + fwd[2] ** 2)
        fwd = [f32(fwd[0] / n), 0.0, f32(fwd[2] / n)]
        right = [fwd[2], 0.0, -fwd[0]]          # up x forward
        pos = [npos[0], f32(npos[1] + 0.5), npos[2]]
        m.car_put(0x330, *pos)
        m.car_put(0x33c, f32(fwd[0] * v0), 0.0, f32(fwd[2] * v0))
        m.car_put(0x364, *right, 0.0, 1.0, 0.0, *fwd)
        m.car_put(0x3a0, 1.0)
        m.car_put(0x3d0, *nfwd)
        m.car_put(0x3b8, *nlat)
        m.car_put(0x3c4, *nup)
        m.uc.mem_write(vh.CAR + 8, struct.pack('<h', node0))
        m.put_i32(vh.CAR + 0xc, -1)
        m.put_i32(vh.CAR + 0x32c, len(BODY_POINTS))
        for k, p in enumerate(BODY_POINTS):
            m.car_put(0x8c + 12 * k, *p)
        for w in range(4):
            base = m.wheel(w)
            wp = WHEEL_POINTS[w]
            m.put(base + 0x40, *[pos[j] + right[j] * wp[0] + fwd[j] * wp[2] for j in range(3)])
            m.put_i32(base + 0x8c, 1)
            m.put_i32(base + 0xc0, 1 if w < 2 else 0)
        for off in (0xd7e, 0xd81, 0xd82):
            m.put_bytes(vh.CAR + off, [gear0])
        ratio = m.get(vh.SPECS + 0x60 + 4 * max(gear0, 2))[0]
        m.car_put(0xdb4, f32(v0 * ratio) if gear0 > 1 else 0.0)
        m.put_i32(vh.CAR + 0xdb0, db0)
        m.car_put(0xd60, f32(v0))
        self.ground(m, nodes)
        rows = []
        for i in range(steps):
            s = sched0 + i
            m.put_u32(G_SCHED, s)
            if s % 2 == 0:
                m.put(vh.SCRATCH + 0x40, *m.car_get(0x330, 3))
                m.call_thiscall(LOCATOR, vh.CAR + 8, vh.SCRATCH + 0x40)
                if m.get_bytes(vh.CAR + 0x11, 1)[0]:
                    k = struct.unpack('<h', m.get_bytes(vh.CAR + 8, 2))[0]
                    r = m.get(nodes + k * 0x50 + 4, 12)
                    m.car_put(0x3b8, *r[9:12])
                    m.car_put(0x3c4, *r[3:6])
                    m.car_put(0x3d0, *r[6:9])
                m.call(READ_INPUTS, vh.CAR, budget=5_000_000)
                m.call(DYNAMICS_TICK, vh.CAR)
                self.ground(m, nodes)
                rows.append(self.row(m, name, i, jun_begin))
            m.call(INTEGRATE_POS, vh.CAR)
            m.call(INTEGRATE_ORI, vh.CAR)
        return rows

    @staticmethod
    def ground(m, nodes):
        m.put(NORMAL, 0.0, 1.0, 0.0)
        m.call(GROUND_FRAME, vh.CAR, NORMAL, struct.unpack('<I', struct.pack('<f', 1.0))[0])
        vx, _, vz = m.car_get(0x33c, 3)
        m.car_put(0x35c, octagonal(vx, vz))
        k = struct.unpack('<h', m.get_bytes(vh.CAR + 8, 2))[0]
        y = f32(m.get(nodes + k * 0x50 + 8)[0] + 0.5)
        dy = f32(y - m.car_get(0x334)[0])
        m.car_put(0x334, y)
        for w in range(4):
            a = m.wheel(w) + 0x44
            m.put(a, f32(m.get(a)[0] + dy))

    @staticmethod
    def row(m, name, i, jun_begin):
        c = vh.CAR
        b = lambda off: m.get_bytes(c + off, 1)[0]  # noqa: E731
        sb = lambda off: struct.unpack('<b', m.get_bytes(c + off, 1))[0]  # noqa: E731
        ii = lambda off: m.get_i32(c + off)[0]  # noqa: E731
        f = lambda off: f'{m.get_u32(c + off):08x}'  # noqa: E731
        j = m.get_u32(c + 0xe94)
        out = [name, str(i), str(struct.unpack('<h', m.get_bytes(c + 8, 2))[0]), str(ii(0xc)),
               str(-1 if j == 0 else (j - jun_begin) // 0x38), str(ii(0xe98)), str(ii(0xe9c)),
               f(0xea0), f(0xeb8), str(ii(0xea4)), str(b(0xd8b)), str(b(0xd9c)), str(b(0xd9d)),
               str(ii(0xda0)), str(sb(0xd82)), str(sb(0xd84)), str(b(0xd7f)), str(b(0xd80)),
               str(ii(0xd94)), f(0xdb4), f(0xd60)]
        for off in (0x330, 0x33c, 0x388, 0x37c):
            out += [f(off + 4 * k) for k in range(3)]
        out.append(str(m.get_u32(G_RAND_STATE)))
        return out


HEAD = ['run', 'step', 'node_08', 'section_0c', 'junction_e94', 'branch_e98', 'class_e9c',
        'skill_ea0', 'target_eb8', 'stuck_ea4', 'event_d8b', 'gas_d9c', 'brake_d9d', 'steer_da0',
        'gear_d82', 'shift_d84', 'gas_d7f', 'brake_d80', 'steer_d94', 'rpm_db4', 'fwd_d60',
        'pos_x', 'pos_y', 'pos_z', 'vel_x', 'vel_y', 'vel_z', 'ang_x', 'ang_y', 'ang_z',
        'f_x', 'f_y', 'f_z', 'rand_state']


def record(game):
    loop = Loop(game)
    rows = [HEAD]
    for r in RUNS:
        rows += loop.run(*r)
        print(r[0], file=sys.stderr)
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    p.add_argument('--out', type=Path, default=vh.ROOT / 'local/fixtures/ai-loop.tsv')
    p.add_argument('--check', action='store_true')
    a = p.parse_args()
    rows = record(a.game_dir)
    if a.check and record(a.game_dir) != rows:
        sys.exit('two recordings differ')
    head = ['# Closed-loop original AI + car model runs (0x474440, 0x4991e0 AI branch, 0x49fd30,',
            '# flat-ground stand-in following node heights, 0x49b7a0, 0x49b8e0); replay-ai-loop.py.']
    a.out.write_text('\n'.join(head + ['\t'.join(r) for r in rows]) + '\n')
    print(f'{len(rows) - 1} rows', file=sys.stderr)


if __name__ == '__main__':
    main()
