"""Record V2 fixtures: the original per-wheel grip and tyre force.

Executes nfs5.exe in Unicorn (scripts/research/vehicle_harness.py, x87 control
word 0x27f) on synthetic cars:

- Car_WheelGripLimits 0x4a1900, cdecl (car): grip w+0xac and alt-table flag
  w+0x80 for the 4 wheels, and car+0x84 (1.16 plus 8x the rear downforce term).
- Wheel_TyreForce 0x4a20d0, cdecl (car, wheel, unused): per wheel force
  w+0x90/w+0x98, skid w+0xb8, locked w+0xbc, and the in-place rewrites of the
  cancel vector w+0x9c/w+0xa4 (steer rotation) and demand w+0xa8 (clamp, x mu);
  w+0x94 is zeroed on the rolling path. It calls Wheel_LockedSlide 0x4a1c70
  and Wheel_FrictionCircle 0x4a1dc0.
- Wheel_LockedSlide 0x4a1c70 and Wheel_FrictionCircle 0x4a1dc0 directly,
  cdecl (car, wheel, unused), so the incoming skid w+0xb8 (zeroed by
  0x4a20d0 before it calls them) and arbitrary w+0x90/w+0x98 are covered.

Field meanings: docs/re/vehicle.md sections 2 and 4 (workspace root).

Cars: carSpecs from the V1 derivation (0x49c82f..0x49cb50, default tuning) for
356Acoupe16 (RWD, tyre type 0), 993coupe436 (C4, AWD) and GT2race (tyre
type 2); written to vehicle-tyre-cars.tsv as the 0x1c4 bytes in hex. The tyre
type sim+0x1c0 is overridden per row (column tyre_type) to cover type 1.

Globals: vehicle_harness.GLOBALS (LCG state/multiplier as shipped, race-start
step 0x1c4, car count 0), with per-row g_SchedStep (read by the res+0x24
branch), g_HandlingModeFlag, g_Throttle [0x628fb0], g_Brake [0x628fac] (not
read by these functions; swept to show that) and g_TrackIsAutobahn. With
the autobahn flag set, the spatial query 0x473330 (thiscall, 2 args, callee
pops) is stubbed to return the row's `offroad` column (0 selects the
alternate grip table).

Damage: Car_DamageTyres 0x493980 / Car_DamageAero 0x493b30 run unstubbed.
A nonzero damage is produced by giving part record 19 the kind (9 or 6) and
its +0x164 base damage; with no zone flags the function returns +0x164.

Every write outside the stack is logged (UC_HOOK_MEM_WRITE) and checked
against the output columns, so the recorded outputs are all the fields the
functions write.

    python scripts/research/replay-vehicle-tyre.py --game-dir local/game \
        --out-dir local/fixtures
"""
import argparse
import math
from pathlib import Path
import random
import struct
import sys

from unicorn import UC_HOOK_MEM_WRITE
from unicorn.x86_const import UC_X86_REG_ESP

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vehicle_harness as vh  # noqa: E402

GRIP_LIMITS, TYRE_FORCE, LOCKED_SLIDE, FRICTION_CIRCLE = 0x4a1900, 0x4a20d0, 0x4a1c70, 0x4a1dc0
TRACK_QUERY = 0x473330
G_THROTTLE, G_BRAKE = 0x628fb0, 0x628fac

CARS = [('356A', '356Acoupe16.sim', '356A'), ('993C4', '993coupe436.sim', '993'),
        ('GT2', 'GT2race.sim', 'Gt2')]

WHEEL0 = 0x7b8
WHEEL_SIZE = 0xc4
LAST_PART = vh.PARTS + 19 * vh.PART_SIZE


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def short(x):
    """Shortest decimal that parses back to the same f32 (Rust `parse::<f32>`)."""
    x = f32(x)
    for p in range(1, 10):
        s = '%.*g' % (p, x)
        if f32(float(s)) == x and (math.copysign(1, float(s)) == math.copysign(1, x)):
            return s
    raise AssertionError(x)


def fmt(v):
    return short(v) if isinstance(v, float) else str(v)


def write_tsv(path, rows, comments):
    header = [k for k, _ in rows[0]]
    lines = [f'# {c}' for c in comments] + ['\t'.join(header)]
    for row in rows:
        if [k for k, _ in row] != header:
            raise RuntimeError(f'column mismatch in {path}')
        lines.append('\t'.join(fmt(v) for _, v in row))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return path.stat().st_size


class Recorder:
    def __init__(self, game_dir):
        module, data = vh.load_module('nfs5.exe', game_dir)
        self.offroad = 1

        def track_query(m):
            # thiscall with 2 stack args: move the return address over them.
            sp = m.uc.reg_read(UC_X86_REG_ESP)
            m.put_u32(sp + 8, m.get_u32(sp))
            m.uc.reg_write(UC_X86_REG_ESP, sp + 8)
            return self.offroad

        self.m = vh.Machine(module, data, stubs={TRACK_QUERY: track_query})
        self.writes = set()
        stack = (vh.STACK_TOP - 0x10000, vh.STACK_TOP)

        def on_write(_uc, _access, address, size, _value, _user):
            if not stack[0] <= address < stack[1]:
                for a in range(address, address + size):
                    self.writes.add(a & ~3)

        self.m.uc.hook_add(UC_HOOK_MEM_WRITE, on_write)
        self.specs = {}
        for name, sim, model in CARS:
            raw = (game_dir / 'GameData/Simulation/CarData' / sim).read_bytes()
            self.specs[name] = vh.derive_specs(self.m, raw, model)

    def setup(self, p):
        m = self.m
        m.reset_car()
        m.set_globals(g_SchedStep=p['step'], g_HandlingModeFlag=p['handling'],
                      g_TrackIsAutobahn=p.get('autobahn', 0))
        m.put(G_THROTTLE, p['throttle'])
        m.put(G_BRAKE, p['brake'])
        m.put_bytes(vh.SPECS, self.specs[p['car']])
        m.put_i32(vh.SPECS + 0x1c0, p['tyre_type'])
        m.put_i32(vh.RES + 0x20, p.get('res20', 0))
        m.put_i32(vh.RES + 0x24, p.get('res24', 0))
        for i in range(4):
            m.put_i32(m.wheel(i) + 0xc0, 1 if i < 2 else 0)
        # Damage through part record 19 (kind at +0x26 as a short).
        dmg = p.get('dmg', 0.0)
        if dmg:
            m.uc.mem_write(LAST_PART + 0x26, struct.pack('<h', p['dmg_kind']))
            m.put(LAST_PART + 0x164, dmg)
        self.offroad = p.get('offroad', 1)

    def run(self, entry, *args):
        self.writes = set()
        self.m.call(entry, *args)
        return self.writes


# ---------------------------------------------------------------------------
# Grip (0x4a1900).
GRIP_CAR_IN = [('normal_y_4a0', 0x4a0), ('ground_up_3a0', 0x3a0), ('accel_lat_d4c', 0xd4c),
               ('accel_long_d54', 0xd54), ('vel_fwd_d60', 0xd60), ('slip_ratio_dc0', 0xdc0)]


def grip_row(rec, p):
    m = rec.m
    rec.setup(p)
    for name, off in GRIP_CAR_IN:
        m.car_put(off, p[name])
    m.put_bytes(vh.CAR + 0xd7f, [p['gas_d7f']])
    m.put_bytes(vh.CAR + 0x52c, [p['flags_52c']])
    m.put_i32(vh.CAR + 0xda8, p['ai_da8'])
    for i in range(4):
        m.put_i32(m.wheel(i) + 0x8c, p['surface'][i])
        m.put(m.wheel(i) + 0xac, 12345.0)          # overwritten
    m.car_put(0x84, -777.0)                           # overwritten
    written = rec.run(GRIP_LIMITS, vh.CAR)
    allowed = {vh.CAR + 0x84} | {m.wheel(i) + o for i in range(4) for o in (0x80, 0xac)}
    if not written <= allowed:
        raise RuntimeError(f'unexpected grip writes {sorted(hex(a) for a in written - allowed)}')
    row = [('case', p['case']), ('car', p['car']), ('tyre_type', p['tyre_type']),
           ('handling', p['handling']), ('autobahn', p['autobahn']), ('offroad', p['offroad']),
           ('dmg_aero', p.get('dmg', 0.0)), ('flags_52c', p['flags_52c']), ('ai_da8', p['ai_da8']),
           ('gas_d7f', p['gas_d7f'])]
    row += [(name, f32(p[name])) for name, _ in GRIP_CAR_IN]
    row += [(f'surface_8c_{i}', p['surface'][i]) for i in range(4)]
    row += [(f'out_alt_80_{i}', m.get_i32(m.wheel(i) + 0x80)[0]) for i in range(4)]
    row += [(f'out_grip_ac_{i}', m.get(m.wheel(i) + 0xac)[0]) for i in range(4)]
    row += [('out_field_84', m.car_get(0x84)[0])]
    return row, written


def grip_cases():
    rng = random.Random(0x4a1900)
    surf_values = list(range(16)) + [0x13, 0x2f, -1]       # & 0xf in the original
    cases = []

    def base(**kw):
        p = dict(case='grip', car='356A', tyre_type=0, handling=0, autobahn=0, offroad=1,
                 flags_52c=0, ai_da8=0, gas_d7f=0, normal_y_4a0=1.0, ground_up_3a0=1.0,
                 accel_lat_d4c=0.0, accel_long_d54=0.0, vel_fwd_d60=0.0, slip_ratio_dc0=0.0,
                 surface=[3, 3, 3, 3], step=0x1c4, throttle=0.0, brake=0.0)
        p.update(kw)
        return p

    # Surface x tyre table, every car and tyre type, standing still.
    for car, _, _ in CARS:
        for tt in (0, 1, 2):
            for s in range(0, 16, 4):
                cases.append(base(case='table', car=car, tyre_type=tt,
                                  surface=[s, s + 1, s + 2, s + 3]))
            cases.append(base(case='table', car=car, tyre_type=tt, surface=surf_values[16:] + [7]))
    # Type swaps: flag 8 / AI, normal and autobahn table.
    for car in ('356A', 'GT2'):
        for tt in (0, 1, 2):
            for flags, ai in ((0, 0), (8, 0), (0, 1), (0xf7, 0)):
                for autobahn, offroad in ((0, 1), (1, 1), (1, 0)):
                    cases.append(base(case='swap', car=car, tyre_type=tt, flags_52c=flags,
                                      ai_da8=ai, autobahn=autobahn, offroad=offroad,
                                      surface=[1, 5, 9, 14]))
    # Load transfer: longitudinal (gas gate, slip ratio) and lateral sign.
    for car, _, _ in CARS:
        for d54 in (-9.5, -3.0, 0.0, 2.5, 8.0):
            for d4c in (-12.0, -0.5, 0.0, 0.5, 12.0):
                for gas, d60, dc0 in ((0, 20.0, 0.0), (200, 20.0, 0.05), (254, -4.0, 0.0),
                                      (1, 0.0, 0.02), (128, 3.0, -0.2)):
                    cases.append(base(case='transfer', car=car, tyre_type=0 if car != 'GT2' else 2,
                                      accel_long_d54=d54, accel_lat_d4c=d4c, gas_d7f=gas,
                                      vel_fwd_d60=d60, slip_ratio_dc0=dc0))
    # Downforce threshold and the 0.939 ground test, handling mode, damage.
    for car, _, _ in CARS:
        for d60 in (4.99, 5.0, 5.01, 30.0, 85.0, -40.0):
            for up in (0.939, 0.94, 1.0):
                for handling in (0, 1):
                    cases.append(base(case='downforce', car=car, vel_fwd_d60=d60, ground_up_3a0=up,
                                      handling=handling, accel_long_d54=1.5, accel_lat_d4c=-2.0,
                                      tyre_type=2 if car == 'GT2' else 0))
        for dmg in (25.0, 150.0):
            cases.append(base(case='damage', car=car, vel_fwd_d60=50.0, dmg=dmg, dmg_kind=6))
    # Random sweep.
    for _ in range(500):
        car = rng.choice(CARS)[0]
        autobahn = rng.random() < 0.2
        cases.append(base(
            case='random', car=car, tyre_type=rng.choice((0, 0, 1, 2)),
            handling=int(rng.random() < 0.2), autobahn=int(autobahn),
            offroad=rng.choice((0, 1)) if autobahn else 1,
            flags_52c=rng.choice((0, 0, 8, 0x18)), ai_da8=int(rng.random() < 0.2),
            gas_d7f=rng.choice((0, 0, 1, 100, 254)),
            normal_y_4a0=rng.choice((1.0, rng.uniform(0.6, 1.0))),
            ground_up_3a0=rng.choice((1.0, rng.uniform(0.5, 1.0))),
            accel_lat_d4c=rng.uniform(-20, 20), accel_long_d54=rng.uniform(-15, 10),
            vel_fwd_d60=rng.choice((0.0, rng.uniform(-10, 90), rng.uniform(0, 8))),
            slip_ratio_dc0=rng.choice((0.0, rng.uniform(-0.3, 0.3))),
            surface=[rng.choice(surf_values) for _ in range(4)],
            dmg=rng.choice((0.0, 0.0, 0.0, rng.uniform(0, 120))), dmg_kind=6))
    return cases


# ---------------------------------------------------------------------------
# Tyre force (0x4a20d0, 0x4a1c70, 0x4a1dc0) on one wheel.
TYRE_WHEEL_IN = [('lat_90', 0x90, 'f'), ('long_98', 0x98, 'f'), ('cancel_lat_9c', 0x9c, 'f'),
                 ('cancel_long_a4', 0xa4, 'f'), ('demand_a8', 0xa8, 'f'), ('grip_ac', 0xac, 'f'),
                 ('steer_b4', 0xb4, 'f'), ('skid_b8', 0xb8, 'f'), ('surface_8c', 0x8c, 'i')]
TYRE_WHEEL_OUT = [('lat_90', 0x90, 'f'), ('field_94', 0x94, 'f'), ('long_98', 0x98, 'f'),
                  ('cancel_lat_9c', 0x9c, 'f'), ('cancel_long_a4', 0xa4, 'f'),
                  ('demand_a8', 0xa8, 'f'), ('skid_b8', 0xb8, 'f'), ('locked_bc', 0xbc, 'i')]
TYRE_CAR_F = [('vel_fwd_d60', 0xd60), ('slip_ratio_dc0', 0xdc0)]
TYRE_CAR_B = [('gas_d7f', 0xd7f), ('brake_d80', 0xd80), ('gear_d82', 0xd82), ('handbrake_d85', 0xd85)]
TYRE_CAR_I = [('ai_da8', 0xda8), ('spin_rear_db8', 0xdb8), ('spin_front_dbc', 0xdbc)]
FUNCS = {'tyre': TYRE_FORCE, 'locked': LOCKED_SLIDE, 'circle': FRICTION_CIRCLE}


def tyre_row(rec, p):
    m = rec.m
    rec.setup(p)
    for name, off in TYRE_CAR_F:
        m.car_put(off, p[name])
    for name, off in TYRE_CAR_B:
        m.put_bytes(vh.CAR + off, [p[name] & 0xff])
    for name, off in TYRE_CAR_I:
        m.put_i32(vh.CAR + off, p[name])
    w = m.wheel(p['wheel'])
    for name, off, kind in TYRE_WHEEL_IN:
        (m.put_i32 if kind == 'i' else m.put)(w + off, p[name])
    m.put(w + 0x94, 99.0)
    m.put_i32(w + 0xbc, 7)
    written = rec.run(FUNCS[p['fn']], vh.CAR, w, 0)
    allowed = {w + off for _, off, _ in TYRE_WHEEL_OUT}
    if not written <= allowed:
        raise RuntimeError(f'unexpected tyre writes {sorted(hex(a - w) for a in written - allowed)}')
    row = [('case', p['case']), ('fn', p['fn']), ('car', p['car']), ('tyre_type', p['tyre_type']),
           ('wheel', p['wheel']), ('handling', p['handling']), ('res20', p['res20']),
           ('res24', p['res24']), ('step', p['step']), ('throttle', f32(p['throttle'])),
           ('brake', f32(p['brake'])), ('dmg_tyres', p.get('dmg', 0.0))]
    row += [(n, f32(p[n])) for n, _ in TYRE_CAR_F]
    row += [(n, p[n]) for n, _ in TYRE_CAR_B + TYRE_CAR_I]
    row += [(n, p[n] if k == 'i' else f32(p[n])) for n, _, k in TYRE_WHEEL_IN]
    for name, off, kind in TYRE_WHEEL_OUT:
        row.append((f'out_{name}', (m.get_i32 if kind == 'i' else m.get)(w + off)[0]))
    return row, {a - w for a in written}


def tyre_cases():
    rng = random.Random(0x4a20d0)
    cases = []

    def base(**kw):
        p = dict(case='', fn='tyre', car='356A', tyre_type=0, wheel=2, handling=0, res20=0, res24=0,
                 step=0x1c4, throttle=0.0, brake=0.0, vel_fwd_d60=0.0, slip_ratio_dc0=0.0,
                 gas_d7f=0, brake_d80=0, gear_d82=2, handbrake_d85=0, ai_da8=0, spin_rear_db8=0,
                 spin_front_dbc=0, lat_90=0.0, long_98=0.0, cancel_lat_9c=0.0, cancel_long_a4=0.0,
                 demand_a8=0.0, grip_ac=6.0, steer_b4=0.0, skid_b8=0.0, surface_8c=3)
        p.update(kw)
        return p

    def moving(v_fwd, v_lat, **kw):
        # Cancel vector as 0x49f160 builds it (no yaw term): -16*v.
        kw.setdefault('slip_ratio_dc0', v_lat / v_fwd if v_fwd > 5 else 0.0)
        return base(vel_fwd_d60=v_fwd, cancel_lat_9c=-16.0 * v_lat, cancel_long_a4=-16.0 * v_fwd, **kw)

    ratios = [0.0, 1e-4, -1e-4, 0.001, -0.002, 0.004, -0.006, 0.01, -0.012, 0.02, -0.03, 0.05,
              -0.08, 0.15, -0.3, 0.6, -1.0, 2.5, -10.0]
    # 1. Slip-angle sweep, rolling, front and rear, every car.
    for car, _, _ in CARS:
        tt = 2 if car == 'GT2' else 0
        for wheel in (0, 1, 2, 3):
            for v in (0.3, 2.0, 10.0, 30.0, 60.0, -8.0):
                for r in ratios:
                    for demand, steer in ((0.0, 0.0), (2.0, 4.0 if wheel < 2 else 0.0),
                                          (-3.0, -9.0 if wheel < 2 else 0.0)):
                        cases.append(moving(v, r * abs(v), case='slip', car=car, tyre_type=tt,
                                            wheel=wheel, demand_a8=demand, steer_b4=steer,
                                            grip_ac=rng.choice((3.0, 5.5, 9.0)),
                                            gas_d7f=200 if demand > 0 else 0,
                                            brake_d80=200 if demand < 0 else 0,
                                            throttle=0.8 if demand > 0 else 0.0,
                                            brake=0.8 if demand < 0 else 0.0))
    # 2. Handbrake: on/off x rear/front x speed around 0.5 x demand inside/over grip.
    for car, _, _ in CARS:
        tt = 2 if car == 'GT2' else 0
        for tyre_type in sorted({tt, 1}):
            for wheel in (0, 3):
                for hb in (0, 1):
                    for v in (0.3, 0.5, 0.51, -0.6, 5.0, 30.0, -20.0):
                        for r in (0.0, 0.03, -0.2):
                            for demand in (0.0, 1.0, -4.0, -30.0):
                                cases.append(moving(v, r * abs(v), case='handbrake', car=car,
                                                    tyre_type=tyre_type, wheel=wheel,
                                                    handbrake_d85=hb, demand_a8=demand,
                                                    grip_ac=7.0, brake_d80=0xff if demand < 0 else 0,
                                                    brake=1.0 if demand < 0 else 0.0,
                                                    steer_b4=6.0 if wheel == 0 else 0.0))
    # 3. Braking clamp and over-grip lock: d80 around 0xeb, d60 around 2*(16-grip),
    #    |d60| around 5 with db8, res+0x24 with step & 3, AI, surface, tyre type 2.
    for _ in range(700):
        grip = rng.choice((4.0, 6.0, 9.5, 14.0, 17.0))
        v = rng.choice((2 * (16 - grip), 2 * (16 - grip) + 0.01, 4.9, 5.0, -5.0, 1.0, 12.0, 24.9,
                        25.0, 40.0, -3.0, rng.uniform(-30, 60)))
        car = rng.choice(CARS)[0]
        cases.append(moving(v, rng.choice((0.0, 0.02, -0.1)) * abs(v), case='lock', car=car,
                            tyre_type=rng.choice((0, 1, 2)), wheel=rng.choice((0, 1, 2, 3)),
                            demand_a8=rng.choice((-1.0, -1.0, 1.0)) * rng.choice((0.5, 3.0, 8.0, 20.0, 60.0)),
                            grip_ac=grip, brake_d80=rng.choice((0, 0xeb, 0xec, 0xff)),
                            gas_d7f=rng.choice((0, 0x80, 0x81, 0xfe)),
                            throttle=rng.choice((0.0, 0.25, 0.26, 1.0)),
                            gear_d82=rng.choice((0, 1, 2, 3, 6)),
                            spin_rear_db8=rng.choice((0, 0, 1, 2)), ai_da8=rng.choice((0, 0, 1)),
                            surface_8c=rng.choice((0, 1, 2, 5)), res24=rng.choice((0, 0, 1)),
                            step=rng.choice((0x1c4, 0x1c5, 0x1c6, 0x1c7)),
                            handbrake_d85=rng.choice((0, 0, 0, 1))))
    # 4. Friction circle: drive over grip, wheelspin flags, res+0x20, handling mode, damage.
    for _ in range(900):
        v = rng.choice((rng.uniform(-10, 70), rng.uniform(0, 6)))
        wheel = rng.choice((0, 1, 2, 3))
        car = rng.choice(CARS)[0]
        cases.append(moving(v, rng.uniform(-0.4, 0.4) * abs(v), case='circle', car=car,
                            tyre_type=rng.choice((0, 1, 2)), wheel=wheel,
                            demand_a8=rng.choice((rng.uniform(-25, 25), rng.uniform(0, 8))),
                            grip_ac=rng.uniform(1, 15), gas_d7f=rng.choice((0, 0x80, 0x81, 0xfe)),
                            throttle=rng.choice((0.0, 0.5, 1.0)), brake_d80=rng.choice((0, 0x80)),
                            gear_d82=rng.choice((1, 2, 3, 4)), spin_rear_db8=rng.choice((0, 1, 2)),
                            spin_front_dbc=rng.choice((0, 2)), res20=rng.choice((0, 0, 1)),
                            slip_ratio_dc0=rng.choice((0.0, 0.1, -0.149, 0.15, 0.5)),
                            ai_da8=rng.choice((0, 0, 1)), surface_8c=rng.choice((0, 1, 2, 7)),
                            handling=rng.choice((0, 0, 1)),
                            steer_b4=rng.uniform(-30, 30) if wheel < 2 else 0.0,
                            dmg=rng.choice((0.0, 0.0, rng.uniform(0, 150))), dmg_kind=9))
    # 5. Fully random tyre calls (raw cancel vectors incl. yaw-term asymmetry).
    for _ in range(700):
        wheel = rng.choice((0, 1, 2, 3))
        cases.append(base(case='random', car=rng.choice(CARS)[0], tyre_type=rng.choice((0, 1, 2)),
                          wheel=wheel, handling=rng.choice((0, 0, 1)), res20=rng.choice((0, 1)),
                          res24=rng.choice((0, 1)), step=rng.choice((0x1c4, 0x1c5, 0x2000)),
                          throttle=rng.choice((0.0, 0.25, 0.7)), brake=rng.choice((0.0, 0.6)),
                          vel_fwd_d60=rng.uniform(-40, 80), slip_ratio_dc0=rng.uniform(-0.3, 0.3),
                          gas_d7f=rng.randrange(256), brake_d80=rng.randrange(256),
                          gear_d82=rng.choice((0, 1, 2, 3, 5, 7)), handbrake_d85=rng.choice((0, 0, 1)),
                          ai_da8=rng.choice((0, 0, 1)), spin_rear_db8=rng.choice((0, 1, 2)),
                          spin_front_dbc=rng.choice((0, 2)),
                          cancel_lat_9c=rng.choice((0.0, rng.uniform(-200, 200), rng.uniform(-5, 5))),
                          cancel_long_a4=rng.choice((0.0, rng.uniform(-1300, 300), rng.uniform(-5, 5))),
                          demand_a8=rng.choice((0.0, rng.uniform(-40, 40))), grip_ac=rng.uniform(0, 18),
                          steer_b4=rng.choice((0.0, rng.uniform(-127, 127))) if wheel < 2 else 0.0,
                          surface_8c=rng.choice((0, 1, 2, 3, 9)),
                          dmg=rng.choice((0.0, 0.0, 0.0, rng.uniform(0, 200))), dmg_kind=9))
    # 6. Exact thresholds: cancel_long 0, |9c|+|a4| == 32, alpha == 1, 2, 16, steer to zero angle.
    for wheel in (0, 2):
        for c9, ca in ((0.0, 0.0), (3.0, 0.0), (-3.0, -0.0), (16.0, -16.0), (-16.0, 16.0),
                       (10.0, -22.0), (-0.1, -31.9), (0.0, -50.0), (1.0, -1000.0)):
            for steer in (0.0, 1e-9, 3.0):
                cases.append(base(case='edge', wheel=wheel, cancel_lat_9c=c9, cancel_long_a4=ca,
                                  vel_fwd_d60=-ca / 16, steer_b4=steer if wheel == 0 else 0.0,
                                  grip_ac=5.0))
        for alpha in (0.5, 1.0, 2.0, 15.99, 16.0, 16.01):
            t = alpha / 256 * 2 * math.pi
            for sgn in (1, -1):
                cases.append(base(case='edge', wheel=wheel, cancel_lat_9c=sgn * 400 * math.tan(t),
                                  cancel_long_a4=-800.0, vel_fwd_d60=50.0, grip_ac=8.0))
    # 7. Wheel_LockedSlide and Wheel_FrictionCircle called directly (incoming skid, raw forces).
    for _ in range(300):
        cases.append(base(case='direct', fn='locked', car=rng.choice(CARS)[0],
                          tyre_type=rng.choice((0, 1, 2)), wheel=rng.choice((0, 2)),
                          cancel_lat_9c=rng.choice((0.0, 0.001, -0.004, rng.uniform(-100, 100))),
                          cancel_long_a4=rng.choice((0.0, 0.002, rng.uniform(-900, 100))),
                          grip_ac=rng.uniform(0, 15), skid_b8=rng.choice((0.0, rng.uniform(0, 30))),
                          dmg=rng.choice((0.0, rng.uniform(0, 100))), dmg_kind=9))
    for _ in range(500):
        cases.append(base(case='direct', fn='circle', car=rng.choice(CARS)[0],
                          tyre_type=rng.choice((0, 1, 2)), wheel=rng.choice((0, 1, 2, 3)),
                          lat_90=rng.uniform(-20, 20), long_98=rng.uniform(-25, 25),
                          grip_ac=rng.uniform(0.5, 15), skid_b8=rng.choice((0.0, rng.uniform(0, 3))),
                          gas_d7f=rng.choice((0, 0x80, 0x81)), spin_rear_db8=rng.choice((0, 2)),
                          spin_front_dbc=rng.choice((0, 2)), res20=rng.choice((0, 1)),
                          slip_ratio_dc0=rng.choice((0.0, 0.2)), ai_da8=rng.choice((0, 1)),
                          surface_8c=rng.choice((1, 2)), gear_d82=rng.choice((2, 3)),
                          handling=rng.choice((0, 1)),
                          dmg=rng.choice((0.0, rng.uniform(0, 100))), dmg_kind=9))
    return cases


def record(game_dir):
    rec = Recorder(game_dir)
    cars = [[('car', name), ('sim', sim), ('model', model), ('specs_hex', rec.specs[name].hex())]
            for name, sim, model in CARS]
    grip, tyre, written = [], [], {}
    for p in grip_cases():
        row, _ = grip_row(rec, p)
        grip.append(row)
    for p in tyre_cases():
        row, w = tyre_row(rec, p)
        tyre.append(row)
        written.setdefault(p['fn'], set()).update(w)
    return cars, grip, tyre, written


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    first = record(args.game_dir)
    second = record(args.game_dir)
    if first[:3] != second[:3]:
        raise RuntimeError('two runs differ')
    cars, grip, tyre, written = first
    note = 'generated by scripts/research/replay-vehicle-tyre.py (x87 control word 0x27f).'
    sizes = [
        write_tsv(args.out_dir / 'vehicle-tyre-cars.tsv', cars, [
            'V2 cars: carSpecs (0x1c4 bytes, hex) from nfs5.exe 0x49c82f..0x49cb50, default tuning;',
            note]),
        write_tsv(args.out_dir / 'vehicle-tyre-grip.tsv', grip, [
            'V2 grip: nfs5.exe Car_WheelGripLimits 0x4a1900 (inputs, then out_* fields written);',
            'tyre_type overrides sim+0x1c0; dmg_aero is Car_DamageAero 0x493b30; ' + note]),
        write_tsv(args.out_dir / 'vehicle-tyre-force.tsv', tyre, [
            'V2 tyre: nfs5.exe fn=tyre Wheel_TyreForce 0x4a20d0, locked Wheel_LockedSlide 0x4a1c70,',
            'circle Wheel_FrictionCircle 0x4a1dc0 on one wheel; inputs, then out_* wheel fields;',
            'tyre_type overrides sim+0x1c0; dmg_tyres is Car_DamageTyres 0x493980; ' + note]),
    ]
    for fn, offs in sorted(written.items()):
        print(f'{fn}: writes w+' + ', w+'.join(f'{o:#x}' for o in sorted(offs)))
    print(f'{len(cars)} cars, {len(grip)} grip rows, {len(tyre)} tyre rows, '
          f'{sum(sizes)} bytes; two runs identical -> {args.out_dir}')


if __name__ == '__main__':
    main()
