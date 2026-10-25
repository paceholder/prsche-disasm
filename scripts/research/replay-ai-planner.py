#!/usr/bin/env python3
"""Record the original AI planners on sampled car states on real tracks
(change original-ai, tasks 4.1/4.3; docs/re/ai.md sections 5-6 in the
workspace root).

Each row is one AI input step as `Car_ReadInputs` 0x4991e0 runs it:
`AI_TargetSpeed` 0x40b650, then `AI_Drive` 0x40bfe0, on a car whose
carSpecs come from `Sim_DeriveFields` (V1 harness) and whose AISpecs and
per-track tables come from the original `Ais_Load` 0x49ca30. The track graph
and racing line are the original's (ai_harness.py). Other cars of the race
order `[0x5e8b88]` and the player `[0x5e8b34]` are separate car structs.
The values after 0x40b650 alone are recorded too (`t_*` columns).

All floats are f32 hex bit patterns; vectors are three columns.
"""
import argparse
import math
from pathlib import Path
import random
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ai_harness import AiMachine, load_module, track_crp, ROOT  # noqa: E402
from vehicle_harness import CAR, RES, SPECS, SPECS_SIZE, RES_SIZE, derive_specs  # noqa: E402

TARGET_SPEED, DRIVE, AIS_LOAD, FILE_OPEN = 0x40b650, 0x40bfe0, 0x49ca30, 0x59d9b0
AIS = SPECS + 0x400
CAR_STRUCT = 0x1300
# (track, reverse, sim file, model)
CONFIGS = [
    ('alps', 0, '356Acoupe16', '356A'), ('alps', 1, '993coupe436', '993'),
    ('monaco1', 0, 'boxster25', 'boxster'), ('autobahn', 0, 'GT2race', 'Gt2'),
    ('castle', 1, '356coupe11', '356'), ('industrial', 0, '959coupe29', '959'),
]
SAMPLES = 700
FAULTS = False
G = {'skill': 0x6573f4, 'reverse': 0x657430, 'mode': 0x6573e8, 'step': 0x606ac4,
     'running': 0x606ac0, 'autobahn': 0x606ae0, 'rand_state': 0x5d1028, 'rand_mul': 0x5d102c,
     'rally': 0x6573f8, 'count': 0x5e8b58, 'player': 0x5e8b34, 'track': 0x657444}
ORDER = 0x5e8b88

# Car fields: (column, offset, kind) kind: f f32, i i32, s i16, b u8, v vec3.
STATE = [
    ('node_08', 0x8, 's'), ('section_0c', 0xc, 'i'), ('branch_e98', 0xe98, 'i'),
    ('class_e9c', 0xe9c, 'i'), ('skill_ea0', 0xea0, 'f'), ('target_eb8', 0xeb8, 'f'),
    ('stuck_ea4', 0xea4, 'i'), ('pull_1078', 0x1078, 'f'), ('speed_35c', 0x35c, 'f'),
    ('fwd_d60', 0xd60, 'f'), ('gear_d82', 0xd82, 'b'), ('gear_req_d7e', 0xd7e, 'b'),
    ('shift_d84', 0xd84, 'b'), ('pos_330', 0x330, 'v'), ('r_364', 0x364, 'v'),
    ('f_37c', 0x37c, 'v'), ('half_w_3a8', 0x3a8, 'f'), ('node_fwd_3d0', 0x3d0, 'v'),
    ('surface_464', 0x464, 'i'), ('dmg_4e4', 0x4e4, 'f'), ('dmg_4ec', 0x4ec, 'f'),
    ('order_528', 0x528, 'i'), ('flags_52c', 0x52c, 'i'), ('progress_af8', 0xaf8, 'f'),
    ('state_b50', 0xb50, 'i'), ('start_db0', 0xdb0, 'i'), ('spin_db8', 0xdb8, 'i'),
    ('hit_1100', 0x1100, 'i'), ('event_d8b', 0xd8b, 'b'), ('gas_d9c', 0xd9c, 'b'),
    ('brake_d9d', 0xd9d, 'b'), ('steer_da0', 0xda0, 'i'),
]
OUT = [n for n, _, _ in STATE] + ['junction_e94']
RES_SKILL = [('res_154', 0x154), ('res_158', 0x158), ('res_15c', 0x15c), ('res_160', 0x160),
             ('res_164', 0x164), ('res_168', 0x168), ('res_16c', 0x16c), ('res_170', 0x170)]


def fb(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def get_field(m, base, off, kind):
    if kind == 'f':
        return [f'{m.get_u32(base + off):08x}']
    if kind == 'v':
        return [f'{m.get_u32(base + off + 4 * k):08x}' for k in range(3)]
    if kind == 'i':
        return [str(m.get_i32(base + off)[0])]
    if kind == 's':
        return [str(struct.unpack('<h', m.get_bytes(base + off, 2))[0])]
    return [str(m.get_bytes(base + off, 1)[0])]


def put_field(m, base, off, kind, v):
    if kind == 'f':
        m.put(base + off, v)
    elif kind == 'v':
        m.put(base + off, *v)
    elif kind == 'i':
        m.put_i32(base + off, v)
    elif kind == 's':
        m.uc.mem_write(base + off, struct.pack('<h', v))
    else:
        m.uc.mem_write(base + off, bytes([v & 0xff]))


def cols(prefix, kind):
    return [prefix] if kind != 'v' else [f'{prefix}_{a}' for a in 'xyz']


def norm(v):
    n = math.sqrt(sum(c * c for c in v)) or 1.0
    return [c / n for c in v]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def record(game):
    module, data = load_module('nfs5.exe', game)
    head = ['track', 'reverse', 'model', 'skill', 'mode', 'step', 'running', 'rand_state',
            'rand_mul'] + [c for n, _, k in STATE for c in cols(n, k)] + ['junction_e94'] \
        + [n for n, _ in RES_SKILL] + ['player_af8', 'player_b00', 'others'] \
        + ['t_' + c for c in ['junction_e94', 'branch_e98', 'section_0c', 'class_e9c',
                              'skill_ea0', 'target_eb8', 'rand_state']] \
        + ['o_' + c for n, _, k in STATE for c in cols(n, k)] + ['o_junction_e94', 'o_rand_state',
                                                                 'o_product']
    rows = [head]
    for track, reverse, sim, model in CONFIGS:
        m = AiMachine(module, data, stubs={FILE_OPEN: lambda mm: mm.file_addr})
        crp = track_crp(game / f'GameData/Track/{track}.crp')
        jnc = game / f'GameData/Track/{track}.jnc'
        obj = m.build_track(crp, jnc.read_bytes() if jnc.exists() else None)
        g = m.dump_track(obj)
        m.set_racing_line((game / f'GameData/Track/racer/work/{track}{reverse}.lsp').read_bytes())
        jun_begin = m.get_u32(m.get_u32(obj + 0x1c))
        nodes = m.get_u32(obj + 0xc)
        n = g['count']
        raw_sim = (game / f'GameData/Simulation/CarData/{sim}.sim').read_bytes()
        derive_specs(m, raw_sim, model)
        m.put_str(G['track'], track, 0x20)
        m.put_u32(G['reverse'], reverse)
        m.put_u32(CAR + 0x764, AIS)
        m.file_addr = m.place_file((game / f'GameData/Simulation/AICarData/{model}.ais').read_bytes())
        m.call(AIS_LOAD, CAR, budget=5_000_000)
        specs = m.get_bytes(SPECS, SPECS_SIZE)
        res = m.get_bytes(RES, RES_SIZE)
        ais = m.get_bytes(AIS, 0x130)
        others_mem = [m.alloc(CAR_STRUCT) for _ in range(4)]
        rnd = random.Random(f'ai-planner-{track}-{reverse}-{model}')

        def node_rec(k):
            return m.get(nodes + k * 0x50 + 4, 12)

        skipped = 0
        for _ in range(SAMPLES):
            m.reset_car()
            m.put_bytes(SPECS, specs)
            m.put_bytes(RES, res)
            m.put_u32(CAR + 0x764, AIS)
            m.put_bytes(AIS, ais)
            k = rnd.randrange(n)
            rec = node_rec(k)
            npos, nfwd, nlat = rec[0:3], rec[6:9], rec[9:12]
            sgn = -1.0 if reverse else 1.0
            yaw = rnd.gauss(0, 0.15) if rnd.random() < 0.9 else rnd.uniform(-math.pi, math.pi)
            f0 = [sgn * c for c in nfwd]
            fx = f0[0] * math.cos(yaw) - f0[2] * math.sin(yaw)
            fz = f0[0] * math.sin(yaw) + f0[2] * math.cos(yaw)
            fwd = norm([fx, f0[1], fz])
            r = norm(cross([0, 1, 0], fwd))
            lat = rnd.uniform(-5, 5)
            pos = [npos[i] + nlat[i] * lat for i in range(3)]
            pos[1] += 0.6
            v = rnd.choice([0.0, 0.5, 1.5, rnd.uniform(0, 70), rnd.uniform(0, 70), rnd.uniform(-5, 5)])
            top = struct.unpack_from('<i', specs, 0x50)[0] - 1
            gear = rnd.choice([0, 1, rnd.randint(2, top), rnd.randint(2, top)])
            sections = g['sections']
            sec_k = next(i for i, s in enumerate(sections) if s[0][0][0] <= k <= s[0][1][0])
            j_end = sections[sec_k][2]
            junction = rnd.choice([-1, j_end, j_end, rnd.randrange(len(g['junctions']))])
            if junction >= 0:
                jcount = len(g['junctions'][junction][0])
                branch = rnd.randrange(max(1, jcount))
            else:
                branch = 0
            state = {
                'node_08': k, 'section_0c': rnd.choice([sec_k, rnd.randrange(len(sections))]),
                'branch_e98': branch, 'class_e9c': rnd.randint(0, 2),
                'skill_ea0': rnd.choice([1.0, rnd.uniform(0.9, 1.1)]),
                'target_eb8': rnd.uniform(0, 60), 'stuck_ea4': rnd.choice([0, 0, 50, 159, 160]),
                'pull_1078': rnd.choice([0.0, 0.0, rnd.uniform(-8, 8)]),
                'speed_35c': abs(v) * rnd.uniform(1.0, 1.2), 'fwd_d60': v,
                'gear_d82': gear, 'gear_req_d7e': gear, 'shift_d84': rnd.choice([0, 0, 3]),
                'pos_330': pos, 'r_364': r, 'f_37c': fwd, 'half_w_3a8': rnd.uniform(0.8, 1.0),
                'node_fwd_3d0': nfwd, 'surface_464': rnd.choice([0, 1, 1, 2, 3, 5, 9]),
                'dmg_4e4': rnd.choice([0.0, 0.0, rnd.uniform(0, 60)]),
                'dmg_4ec': rnd.choice([0.0, 0.0, rnd.uniform(0, 60)]),
                'order_528': rnd.randint(0, 3), 'flags_52c': 1 if rnd.random() < 0.85 else 0,
                'progress_af8': rnd.uniform(0, 3),
                'state_b50': rnd.choice([0] * 8 + [2, 3]),
                'start_db0': rnd.choice([0, 100, 100, 100]), 'spin_db8': rnd.choice([0, 0, 1, 2]),
                'hit_1100': rnd.choice([0, 0x30, 0xc0]), 'event_d8b': rnd.choice([0, 0, 5]),
                'gas_d9c': 0, 'brake_d9d': 0, 'steer_da0': 0,
            }
            for name, off, kind in STATE:
                put_field(m, CAR, off, kind, state[name])
            m.put_u32(CAR + 0xe94, 0 if junction < 0 else jun_begin + 0x38 * junction)
            skills = [rnd.choice([1.0, rnd.uniform(0.3, 1.0)]) for _ in range(4)] + \
                     [rnd.choice([0.0, rnd.uniform(0, 0.1)]) for _ in range(4)]
            for (_, off), val in zip(RES_SKILL, skills):
                m.put(RES + off, val)
            # Race order: self at order_528, three other cars.
            order = []
            others_desc = []
            oi = 0
            for slot in range(4):
                if slot == state['order_528']:
                    order.append(CAR)
                    continue
                o = others_mem[oi]
                oi += 1
                m.uc.mem_write(o, bytes(CAR_STRUCT))
                d = rnd.uniform(-5, 30)
                ol = rnd.uniform(-3, 3)
                opos = [pos[i] + fwd[i] * d + r[i] * ol for i in range(3)]
                ov = max(0.0, v + rnd.uniform(-8, 4))
                onode = max(0, min(n - 1, k + int(d / 6)))
                m.put(o + 0x330, *opos)
                m.put(o + 0x3a8, rnd.uniform(0.8, 1.0))
                m.put(o + 0xd60, ov)
                m.uc.mem_write(o + 8, struct.pack('<h', onode))
                order.append(o)
                others_desc.append(':'.join([f'{fb(c):08x}' for c in opos]
                                            + [f'{m.get_u32(o + 0x3a8):08x}', f'{fb(ov):08x}', str(onode)]))
            for i, c in enumerate(order):
                m.put_u32(ORDER + 4 * i, c)
            player = others_mem[3]
            m.uc.mem_write(player, bytes(CAR_STRUCT))
            paf8, pb00 = rnd.uniform(0, 3), rnd.uniform(0, 0.5)
            m.put(player + 0xaf8, paf8)
            m.put(player + 0xb00, pb00)
            glob = {'skill': rnd.choice([0, 1, 2]), 'mode': rnd.choice([0, 0, 3]),
                    'step': rnd.choice([0x100, 0x1c1, 0x390, 0x1000, 0x4000]), 'running': 1,
                    'autobahn': 0, 'rand_state': rnd.randrange(1 << 16),
                    'rand_mul': rnd.choice([1, 0x343fd]), 'rally': 0, 'count': 4,
                    'player': player}
            for key, val in glob.items():
                m.put_u32(G[key], val)
            pre = [track, str(reverse), model] + [str(glob[x]) for x in
                                                  ('skill', 'mode', 'step', 'running',
                                                   'rand_state', 'rand_mul')]
            inputs = [c for name, off, kind in STATE for c in get_field(m, CAR, off, kind)]
            inputs += [str(junction)] + [f'{m.get_u32(RES + off):08x}' for _, off in RES_SKILL]
            inputs += [f'{m.get_u32(player + 0xaf8):08x}', f'{m.get_u32(player + 0xb00):08x}',
                       ' '.join(others_desc)]

            def jidx():
                p = m.get_u32(CAR + 0xe94)
                return str(-1 if p == 0 else (p - jun_begin) // 0x38)
            try:
                m.call(TARGET_SPEED, CAR, budget=5_000_000)
            except RuntimeError as err:
                skipped += 1        # stale junction slot with builder stack data
                if FAULTS:
                    print('fault', junction, branch, k, err, file=sys.stderr)
                continue
            mid = [jidx()] + [c for name, off, kind in STATE if name in
                              ('branch_e98', 'section_0c', 'class_e9c', 'skill_ea0', 'target_eb8')
                              for c in get_field(m, CAR, off, kind)]
            # keep column order of the header
            mid = [mid[0], mid[2], mid[1], mid[3], mid[4], mid[5]] + [str(m.get_u32(G['rand_state']))]
            try:
                m.call(DRIVE, CAR, budget=5_000_000)
            except RuntimeError:
                skipped += 1
                continue
            out = [c for name, off, kind in STATE for c in get_field(m, CAR, off, kind)]
            out += [jidx(), str(m.get_u32(G['rand_state'])), str(m.get_u32(0x655a0c))]
            rows.append(pre + inputs + mid + out)
        print(track, reverse, model, 'skipped', skipped, file=sys.stderr)
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    p.add_argument('--out', type=Path, default=ROOT / 'local/fixtures/ai-planner.tsv')
    p.add_argument('--check', action='store_true', help='record twice and compare')
    a = p.parse_args()
    rows = record(a.game_dir)
    if a.check and record(a.game_dir) != rows:
        sys.exit('two recordings differ')
    head = ['# Original AI_TargetSpeed 0x40b650 + AI_Drive 0x40bfe0 on sampled car states;',
            '# recorded by scripts/research/replay-ai-planner.py. Floats as f32 hex bits.']
    a.out.write_text('\n'.join(head + ['\t'.join(r) for r in rows]) + '\n')
    print(f'{len(rows) - 1} rows', file=sys.stderr)


if __name__ == '__main__':
    main()
