"""Record V3 fixtures: the original engine and automatic gearbox.

Executes nfs5.exe in Unicorn (scripts/research/vehicle_harness.py, x87 control
word 0x27f) on synthetic cars:

- Car_EngineDriveForce 0x49e440, cdecl (car), returns the drive force minus
  the cubic drag in ST0 (the caller 0x49f6f9 stores it as f32; the fixture
  keeps the f64 ST0 value). It advances the LCG once per call (misfire draw),
  once more on a rev-limiter hit, calls Car_DraftFactor 0x49d9b0, the damage
  helpers Car_DamageEngine 0x4938a0 (twice) / Car_DamageGearbox 0x493aa0,
  __ftol 0x5a0f98 and, when the clutch is engaged and the car is automatic
  ([0x658098 + player*0x580] == 1) or AI, Car_AutoGearbox 0x49daf0.
- Car_AutoGearbox 0x49daf0, cdecl (car), void, also called directly.
- Car_DraftFactor 0x49d9b0, cdecl (car): with the car count [0x5e8ae8] = 0
  the loop is skipped and it returns 1.0 - 0.0.

Field meanings: docs/re/vehicle.md section 3 (workspace root).

Cars: carSpecs from the V1 derivation (0x49c82f..0x49cb50, default tuning)
for 356Acoupe16 (RWD, 4 gears), 993coupe436 (C4, AWD, 6 gears), boxster25
(RWD, 5 gears) and GT2race (turbo, engine type 1); vehicle-engine-cars.tsv
holds the 0x1c4 bytes in hex. The engine type sim+0x4c is overridden per row
(column engine_type) to cover type 2 (no shipped car has it).

Inputs per row: the car fields read (rpm db4, gear d82/d81/d83/d84, request
d7e, raw gas d7c, brake d80, forward velocity d60, speed +0x35c, slip ratio
dc0, rear/front spin flags db8/dbc, limiter cut +0x548, boost +0x54c, flag
byte +0x52c, AI +0xda8, rear/front grips w+0xac), the globals g_Throttle
[0x628fb0], g_RearContact [0x628f98], g_SchedStep [0x606ac4],
g_HandlingModeFlag [0x6573f8], the auto flag [0x658098] (player +0x520 = 0),
the LCG state [0x5d1028], multiplier [0x5d102c] and last product [0x655a0c].
Damage: Car_DamageEngine returns car+0x4e8 (no kind-1 parts: sum/1), and
Car_DamageGearbox returns part record 19's +0x164 when its kind is 3.

Every write outside the stack is logged (UC_HOOK_MEM_WRITE) and checked
against the output columns, so the recorded outputs are all the fields the
functions write.

Sequences call 0x49e440 repeatedly on one car. Between calls the script plays
the parts of Car_ApplyControls 0x49dd20 that the engine depends on: the shift
timer d84 decrements once per step (when > 0), scripted gear events set d81 =
old gear, d82 = new gear, d84 = sim+0x54 and d83 = 1 for a downshift into a
forward gear (manual box), d7e follows d82, and g_Throttle/d7c/d80 come from
the script every step (the limiter cut zeroes g_Throttle). The speed trace is
v += F/32 with F the returned force as f32 (traction limited to the grip sum
where the column says so), d60 = v, +0x35c = |v|.

    python scripts/research/replay-vehicle-engine.py --game-dir local/game \
        --out-dir local/fixtures
"""
import argparse
import math
from pathlib import Path
import random
import struct
import sys

from unicorn import UC_HOOK_MEM_WRITE
from unicorn.x86_const import UC_X86_REG_FP0, UC_X86_REG_FPSW

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vehicle_harness as vh  # noqa: E402

ENGINE, AUTO_GEARBOX, DRAFT = 0x49e440, 0x49daf0, 0x49d9b0
G_THROTTLE, G_REAR_CONTACT, G_RAND_LAST = 0x628fb0, 0x628f98, 0x655a0c
G_RAND_STATE = 0x5d1028
LAST_PART = vh.PARTS + 19 * vh.PART_SIZE

CARS = [('356A', '356Acoupe16.sim', '356A'), ('993C4', '993coupe436.sim', '993'),
        ('Boxster', 'boxster25.sim', 'Boxster'), ('GT2', 'GT2race.sim', 'Gt2')]

GRIP_OFFS = [0x7b8 + i * 0xc4 + 0xac for i in range(4)]   # 0x864, 0x928, 0x9ec, 0xab0

# (column, car offset, kind): f f32, i i32, b u8, s i8
CAR_FIELDS = [
    ('speed_35c', 0x35c, 'f'), ('flags_52c', 0x52c, 'b'), ('limiter_548', 0x548, 'i'),
    ('boost_54c', 0x54c, 'f'), ('vel_fwd_d60', 0xd60, 'f'), ('gas_d7c', 0xd7c, 'b'),
    ('gear_req_d7e', 0xd7e, 's'), ('brake_d80', 0xd80, 'b'), ('prev_gear_d81', 0xd81, 's'),
    ('gear_d82', 0xd82, 's'), ('downshift_d83', 0xd83, 'b'), ('shift_timer_d84', 0xd84, 's'),
    ('ai_da8', 0xda8, 'i'), ('rpm_db4', 0xdb4, 'f'), ('spin_rear_db8', 0xdb8, 'i'),
    ('spin_front_dbc', 0xdbc, 'i'), ('slip_ratio_dc0', 0xdc0, 'f'),
] + [(f'grip_{i}', off, 'f') for i, off in enumerate(GRIP_OFFS)]
OUT_FIELDS = [('limiter_548', 0x548, 'i'), ('boost_54c', 0x54c, 'f'), ('prev_gear_d81', 0xd81, 's'),
              ('gear_d82', 0xd82, 's'), ('downshift_d83', 0xd83, 'b'), ('shift_timer_d84', 0xd84, 's'),
              ('rpm_db4', 0xdb4, 'f'), ('spin_rear_db8', 0xdb8, 'i'), ('spin_front_dbc', 0xdbc, 'i')]
GLOBAL_IN = ['throttle', 'rear_contact', 'sched_step', 'handling', 'auto', 'rand_state',
             'rand_mul', 'rand_last']
SETUP = ['engine_type', 'dmg_engine', 'dmg_gearbox']


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


class F64(float):
    """Column written with full f64 precision (repr round-trips)."""


def fmt(v):
    if isinstance(v, F64):
        return repr(float(v))
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


def st0(uc):
    """ST(0) as a float (Unicorn's FP0..7 are the physical registers)."""
    top = (uc.reg_read(UC_X86_REG_FPSW) >> 11) & 7
    mant, exp = uc.reg_read(UC_X86_REG_FP0 + top)
    sign = -1.0 if exp & 0x8000 else 1.0
    exp &= 0x7fff
    if exp == 0 and mant == 0:
        return sign * 0.0
    if exp == 0x7fff:
        raise RuntimeError('ST0 is inf/nan')
    value = math.ldexp(mant, exp - 16383 - 63)
    if math.ldexp(float(mant), 0) != mant:
        raise RuntimeError('ST0 has more than 53 mantissa bits')
    return sign * value


class Recorder:
    def __init__(self, game_dir):
        module, data = vh.load_module('nfs5.exe', game_dir)
        self.m = vh.Machine(module, data, trace_calls=True)
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
        self.fields = {name: dict(vh.unpack_fields(self.specs[name], vh.CARSPECS_FIELDS))
                       for name, _, _ in CARS}
        self.allowed = ({vh.CAR + (off & ~3) for _, off, _ in OUT_FIELDS}
                        | {G_THROTTLE, G_RAND_STATE, G_RAND_LAST})

    def spec(self, car, name):
        return self.fields[car][name]

    def setup(self, p):
        m = self.m
        m.reset_car()
        m.set_globals(g_SchedStep=p['sched_step'], g_HandlingModeFlag=p['handling'],
                      g_AutoBox=p['auto'], g_RandState=p['rand_state'], g_RandMul=p['rand_mul'])
        m.put_u32(G_RAND_LAST, p['rand_last'])
        m.put(G_THROTTLE, p['throttle'])
        m.put_u32(G_REAR_CONTACT, p['rear_contact'])
        m.put_bytes(vh.SPECS, self.specs[p['car']])
        m.put_i32(vh.SPECS + 0x4c, p['engine_type'])
        m.car_put(0x4e8, p['dmg_engine'])
        if p['dmg_gearbox']:
            m.uc.mem_write(LAST_PART + 0x26, struct.pack('<h', 3))
            m.put(LAST_PART + 0x164, p['dmg_gearbox'])
        for name, off, kind in CAR_FIELDS:
            v = p[name]
            if kind == 'f':
                m.put(vh.CAR + off, v)
            elif kind == 'i':
                m.put_i32(vh.CAR + off, v)
            else:
                m.put_bytes(vh.CAR + off, [v & 0xff])

    def read_out(self):
        m = self.m
        out = {}
        for name, off, kind in OUT_FIELDS:
            if kind == 'f':
                out[name] = m.get(vh.CAR + off)[0]
            elif kind == 'i':
                out[name] = m.get_i32(vh.CAR + off)[0]
            else:
                b = m.get_bytes(vh.CAR + off, 1)[0]
                out[name] = b - 256 if kind == 's' and b >= 128 else b
        out['throttle'] = m.get(G_THROTTLE)[0]
        out['rand_state'] = m.get_u32(G_RAND_STATE)
        out['rand_last'] = m.get_u32(G_RAND_LAST)
        return out

    def run(self, entry, p):
        self.setup(p)
        self.writes = set()
        self.m.call(entry, vh.CAR)
        if not self.writes <= self.allowed:
            raise RuntimeError(f'unexpected writes {sorted(hex(a) for a in self.writes - self.allowed)}')
        out = self.read_out()
        if entry != AUTO_GEARBOX:
            out['force'] = F64(st0(self.m.uc))
        return out


def in_columns(p):
    row = [('case', p['case']), ('car', p['car'])]
    row += [(k, f32(p[k]) if k in ('dmg_engine', 'dmg_gearbox') else p[k]) for k in SETUP]
    row += [(k, f32(p[k]) if k == 'throttle' else p[k]) for k in GLOBAL_IN]
    row += [(name, f32(p[name]) if kind == 'f' else p[name]) for name, _, kind in CAR_FIELDS]
    return row


def out_columns(out, force=True):
    row = [('out_force', out['force'])] if force else []
    row += [('out_throttle', out['throttle']), ('out_rand_state', out['rand_state']),
            ('out_rand_last', out['rand_last'])]
    row += [(f'out_{name}', out[name]) for name, _, _ in OUT_FIELDS]
    return row


def base(car, **kw):
    p = dict(case='', car=car, engine_type=None, dmg_engine=0.0, dmg_gearbox=0.0, throttle=0.0,
             rear_contact=1, sched_step=0x1c4, handling=0, auto=1, rand_state=0xf874af01,
             rand_mul=1, rand_last=0, speed_35c=0.0, flags_52c=0, limiter_548=0, boost_54c=0.0,
             vel_fwd_d60=0.0, gas_d7c=0, gear_req_d7e=2, brake_d80=0, prev_gear_d81=1, gear_d82=2,
             downshift_d83=0, shift_timer_d84=0, ai_da8=0, rpm_db4=0.0, spin_rear_db8=0,
             spin_front_dbc=0, slip_ratio_dc0=0.0, grip_0=5.0, grip_1=5.0, grip_2=5.0, grip_3=5.0)
    p.update(kw)
    return p


# ---------------------------------------------------------------------------
# (a) single steps.
def grid_cases(rec):
    rng = random.Random(0x49e440)
    cases = []
    for car, _, _ in CARS:
        red = rec.spec(car, 'redline_94')
        top = rec.spec(car, 'gear_count_50') - 1
        etype = rec.spec(car, 'engine_type_4c')
        for gear in range(0, top + 1):
            ratio = rec.spec(car, f'gear_ratio_60_{gear}')
            for rpm in (0.0, 0.45 * red, 0.85 * red, red - 1, red, red + 120, red + 250, 1.3 * red):
                for wheel in (0.0, 0.5, 0.92, 1.03, 1.12, -0.3):
                    for thr in (0.0, 0.4, 1.0):
                        if ratio != 0:
                            v = wheel * red / ratio
                        else:
                            v = wheel * 40.0
                        gas = int(thr * 254)
                        grips = [rng.choice((1.0, 4.0, 9.0)) for _ in range(4)]
                        cases.append(base(
                            car, case='grid', engine_type=etype, throttle=thr, gas_d7c=gas,
                            gear_d82=gear, gear_req_d7e=gear, prev_gear_d81=max(gear - 1, 0),
                            rpm_db4=rpm, vel_fwd_d60=v, speed_35c=abs(v) * rng.choice((1.0, 1.0, 1.05)),
                            slip_ratio_dc0=rng.choice((0.0, 0.05, -0.2)),
                            rand_state=rng.randrange(1 << 16), rand_mul=rng.choice((1, 0x41c64e6d)),
                            brake_d80=rng.choice((0, 0, 0x41)),
                            grip_0=grips[0], grip_1=grips[1], grip_2=grips[2], grip_3=grips[3]))
    return cases


def random_cases(rec):
    rng = random.Random(0x49daf0)
    cases = []
    for _ in range(1000):
        car = rng.choice(CARS)[0]
        red = rec.spec(car, 'redline_94')
        top = rec.spec(car, 'gear_count_50') - 1
        gear = rng.randrange(0, top + 1)
        ratio = rec.spec(car, f'gear_ratio_60_{gear}') or 50.0
        thr = rng.choice((0.0, 0.25, 0.5, 0.5000001, 0.75, 1.0, rng.random()))
        v = rng.choice((rng.uniform(-0.3, 1.3) * red / ratio, rng.uniform(-0.2, 0.2), 0.0,
                        rng.uniform(-10, 90)))
        rpm = rng.choice((rng.uniform(0, 1.3 * red), red + rng.choice((0, 49, 50, 249.9, 250, 499, 500)),
                          -rng.uniform(0, 500), 0.0, abs(v * ratio) + rng.uniform(-300, 300)))
        cases.append(base(
            car, case='random', engine_type=rng.choice((0, 1, 2, rec.spec(car, 'engine_type_4c'))),
            dmg_engine=rng.choice((0.0, 0.0, 0.0, rng.uniform(0, 200))),
            dmg_gearbox=rng.choice((0.0, 0.0, 0.0, rng.uniform(0, 300))),
            throttle=thr, gas_d7c=rng.choice((int(thr * 254), rng.randrange(256))),
            rear_contact=rng.choice((1, 1, 1, 0)),
            sched_step=rng.choice((0x1c4, 0x19f, 0x1a0, 0x100)), handling=rng.choice((0, 0, 1)),
            auto=rng.choice((1, 1, 0)), ai_da8=rng.choice((0, 0, 0, 1)),
            rand_state=rng.choice((0xf874af01, rng.randrange(1 << 32), rng.randrange(1 << 16))),
            rand_mul=rng.choice((1, 0x41c64e6d, rng.randrange(1 << 32) | 1)),
            rand_last=rng.randrange(1 << 32),
            speed_35c=abs(v) * rng.choice((1.0, 1.0, 0.9, 1.2)), flags_52c=rng.choice((0, 0, 4, 0xff)),
            limiter_548=rng.choice((0, 0, 0, 1, 3, 25)), boost_54c=rng.choice((0.0, rng.random())),
            vel_fwd_d60=v, gear_req_d7e=rng.choice((gear, gear - 1 if gear > 2 else gear, rng.randrange(8))),
            brake_d80=rng.choice((0, 0x40, 0x41, 0xff)), prev_gear_d81=rng.randrange(0, top + 1),
            gear_d82=gear, downshift_d83=rng.choice((0, 1)),
            shift_timer_d84=rng.choice((0, 0, 0, 1, 4, -1)), rpm_db4=rpm,
            spin_rear_db8=rng.choice((0, 0, 1, 2)), spin_front_dbc=rng.choice((0, 2)),
            slip_ratio_dc0=rng.choice((0.0, rng.uniform(-0.5, 0.5), 0.1, -0.1)),
            grip_0=rng.uniform(0, 12), grip_1=rng.uniform(0, 12), grip_2=rng.uniform(0, 12),
            grip_3=rng.uniform(0, 12)))
    return cases


def f32_neighbours(x, n):
    """x and the n f32 values on each side of it."""
    bits = struct.unpack('<I', struct.pack('<f', f32(x)))[0]
    return [struct.unpack('<f', struct.pack('<I', b))[0] for b in range(bits - n, bits + n + 1)]


def edge_cases(rec):
    """Exact branch thresholds: T == redline - 300 (lag zeroing), S = db4 -
    |W| at +-200 and 250, |W| at redline + 500, rpm at the boost knees."""
    rng = random.Random(0x49ee54)
    cases = []
    for car, _, _ in CARS:
        red = rec.spec(car, 'redline_94')
        for gear in (2, 3):
            ratio = rec.spec(car, f'gear_ratio_60_{gear}')
            for thr in f32_neighbours((red - 300) / (red + 100), 3):
                target = f32((red + 100) * thr)
                for dw in (-124.0, 0.0, 124.0, 126.0):
                    v = f32((target + dw) / ratio)
                    w = f32(v * ratio)
                    cases.append(base(car, case='edge', engine_type=0, throttle=thr,
                                      gas_d7c=int(thr * 254), gear_d82=gear, gear_req_d7e=gear,
                                      vel_fwd_d60=v, speed_35c=v, rpm_db4=f32(w + rng.choice((0.0, 150.0)))))
            for ds in (-200.0, 200.0, 250.0, -250.0):
                v = f32(rng.uniform(0.3, 0.8) * red / ratio)
                w = f32(v * ratio)
                for rpm in f32_neighbours(w + ds, 2):
                    for thr in (0.4, 1.0):
                        cases.append(base(car, case='edge', engine_type=0, throttle=thr,
                                          gas_d7c=int(thr * 254), gear_d82=gear, gear_req_d7e=gear,
                                          vel_fwd_d60=v, speed_35c=v, rpm_db4=rpm))
            for v in f32_neighbours((red + 500) / ratio, 2):
                cases.append(base(car, case='edge', engine_type=0, throttle=0.0, gear_d82=gear,
                                  gear_req_d7e=gear, vel_fwd_d60=v, speed_35c=v, rpm_db4=red - 400))
        for kind in (1, 2):
            for knee in (0.25, 0.5, 1 / 6, 2 / 3):
                for rpm in f32_neighbours(red * knee, 1):
                    cases.append(base(car, case='edge', engine_type=kind, throttle=0.7, gas_d7c=177,
                                      gear_d82=1, rpm_db4=rpm, boost_54c=rng.random()))
    return cases


def gearbox_cases(rec):
    """Direct Car_AutoGearbox calls around the shift thresholds."""
    rng = random.Random(0x49dd20)
    cases = []
    for _ in range(900):
        car = rng.choice(CARS)[0]
        red = rec.spec(car, 'redline_94')
        top = rec.spec(car, 'gear_count_50') - 1
        gear = rng.randrange(0, top + 1)
        ratio = rec.spec(car, f'gear_ratio_60_{gear}') or 50.0
        down = gear - 1 if gear >= 3 else gear
        dratio = rec.spec(car, f'gear_ratio_60_{down}') or 50.0
        v = rng.choice((rng.uniform(0.85, 1.05) * red / ratio, rng.uniform(0.7, 0.9) * red / dratio,
                        rng.uniform(0, 1.3) * red / ratio))
        sgn = rng.choice((1, 1, 1, -1))
        cases.append(base(
            car, case='gearbox', engine_type=rec.spec(car, 'engine_type_4c'),
            throttle=rng.choice((0.0, 0.5, 0.50000006, 1.0, rng.random())),
            rand_state=rng.randrange(1 << 32), rand_mul=rng.choice((1, 0x41c64e6d, rng.randrange(1 << 32))),
            rand_last=rng.randrange(1 << 32), speed_35c=v, vel_fwd_d60=sgn * v * rng.choice((1.0, 0.98)),
            gear_d82=gear, prev_gear_d81=rng.randrange(0, top + 1), downshift_d83=rng.choice((0, 1)),
            shift_timer_d84=rng.choice((0, 2)),
            rpm_db4=rng.choice((v * ratio, v * ratio * rng.uniform(0.8, 1.3), rng.uniform(0, 1.3 * red))),
            slip_ratio_dc0=rng.choice((0.0, 0.0999, 0.1, -0.1, rng.uniform(-0.4, 0.4)))))
    return cases


# ---------------------------------------------------------------------------
# (b) sequences.
class Sequence:
    """Repeated 0x49e440 calls on one car with the scripted controls."""

    def __init__(self, rec, name, car, rows, **kw):
        self.rec, self.name, self.car, self.rows = rec, name, car, rows
        self.p = base(car, case=name, engine_type=rec.spec(car, 'engine_type_4c'), **kw)
        self.v = self.p['vel_fwd_d60']
        self.n = 0

    def shift(self, gear, manual=True):
        p = self.p
        old = p['gear_d82']
        p['prev_gear_d81'] = old
        p['gear_d82'] = gear
        p['shift_timer_d84'] = self.rec.spec(self.car, 'shift_time_54')
        p['downshift_d83'] = 1 if (gear < old and gear >= 2) else 0
        p['gear_req_d7e'] = gear

    def step(self, throttle, brake=0, event='-', traction=False, shift_to=None):
        p = self.p
        if p['shift_timer_d84'] > 0:
            p['shift_timer_d84'] -= 1
        if shift_to is not None:
            self.shift(shift_to)
            event = f'shift{shift_to}'
        p['gear_req_d7e'] = p['gear_d82']
        p['throttle'] = throttle
        p['gas_d7c'] = int(throttle * 254)
        p['brake_d80'] = brake
        p['vel_fwd_d60'] = self.v
        p['speed_35c'] = abs(self.v)
        out = self.rec.run(ENGINE, p)
        row = [('step', self.n), ('event', event), ('traction', int(traction))] + in_columns(p) + out_columns(out)
        self.rows.append(row)
        for name, _, _ in OUT_FIELDS:
            p[name] = out[name]
        p['rand_state'], p['rand_last'] = out['rand_state'], out['rand_last']
        force = f32(out['force'])
        if traction:
            limit = f32(p['grip_0'] + p['grip_1'] + p['grip_2'] + p['grip_3'])
            force = max(min(force, limit), -limit)
        self.v = f32(self.v + force / 32.0)
        self.n += 1
        return out


def sequences(rec):
    rows = []
    # Launch with wheelspin: rev in N, drop into 1st, low rear grip, auto box.
    for car, grip in (('Boxster', 2.0), ('993C4', 2.0), ('GT2', 3.0)):
        s = Sequence(rec, 'launch_spin', car, rows, gear_d82=1, prev_gear_d81=1,
                     grip_0=grip, grip_1=grip, grip_2=grip, grip_3=grip)
        for _ in range(24):
            s.step(1.0)
        s.step(1.0, shift_to=2)
        for _ in range(260):
            s.step(1.0, traction=True)
    # Full-throttle run through all gears from a standstill in 1st.
    for car in ('Boxster', '356A'):
        s = Sequence(rec, 'full_run', car, rows, grip_0=9.0, grip_1=9.0, grip_2=9.0, grip_3=9.0)
        for _ in range(1700 if car == 'Boxster' else 450):
            s.step(1.0, traction=True)
        # Lift-off: engine braking and downshifts, then a kick-down.
        for _ in range(120):
            s.step(0.0)
        for _ in range(30):
            s.step(0.3)
        for _ in range(60):
            s.step(1.0)
    # Kick-down from a cruise in a high gear (993 C4, AWD).
    s = Sequence(rec, 'kick_down', '993C4', rows, gear_d82=6, prev_gear_d81=5, vel_fwd_d60=30.0,
                 rpm_db4=30.0 * rec.spec('993C4', 'gear_ratio_60_6'), grip_0=8.0, grip_1=8.0,
                 grip_2=8.0, grip_3=8.0)
    for _ in range(40):
        s.step(0.3)
    for _ in range(80):
        s.step(1.0, traction=True)
    # Rev limiter with the manual box (in gear), and in N before/after step 0x1a0.
    for car in ('Boxster', 'GT2'):
        s = Sequence(rec, 'limiter', car, rows, auto=0, rand_mul=0x41c64e6d, rand_state=0x1234,
                     grip_0=9.0, grip_1=9.0, grip_2=9.0, grip_3=9.0)
        for _ in range(140):
            s.step(1.0, traction=True)
        s.step(1.0, shift_to=3)
        for _ in range(40):
            s.step(1.0, traction=True)
    for step in (0x100, 0x1c4):
        s = Sequence(rec, 'limiter_n', 'Boxster', rows, auto=0, gear_d82=1, sched_step=step,
                     rand_mul=0x41c64e6d, rand_state=0x9876)
        for _ in range(80):
            s.step(1.0)
    # Shift timer: manual up- and downshifts with blips (brake on/off).
    s = Sequence(rec, 'shift_timer', '993C4', rows, auto=0, gear_d82=4, prev_gear_d81=3,
                 vel_fwd_d60=25.0, rpm_db4=25.0 * rec.spec('993C4', 'gear_ratio_60_4'),
                 grip_0=8.0, grip_1=8.0, grip_2=8.0, grip_3=8.0)
    for gear, thr, brake in ((5, 1.0, 0), (4, 0.0, 0), (3, 0.0, 0xff), (2, 0.0, 0xff), (3, 1.0, 0),
                             (1, 0.5, 0), (2, 1.0, 0), (0, 0.0, 0)):
        s.step(thr, brake, shift_to=gear)
        for _ in range(11):
            s.step(thr, brake)
    return rows


def record(game_dir):
    rec = Recorder(game_dir)
    cars = [[('car', name), ('sim', sim), ('model', model), ('specs_hex', rec.specs[name].hex())]
            for name, sim, model in CARS]
    single = []
    for p in grid_cases(rec) + random_cases(rec) + edge_cases(rec):
        if p['engine_type'] is None:
            p['engine_type'] = rec.spec(p['car'], 'engine_type_4c')
        single.append(in_columns(p) + out_columns(rec.run(ENGINE, p)))
    gearbox = [in_columns(p) + out_columns(rec.run(AUTO_GEARBOX, p), force=False)
               for p in gearbox_cases(rec)]
    seq = sequences(rec)
    # Car_DraftFactor with no cars.
    p = base('Boxster', engine_type=0, vel_fwd_d60=40.0, speed_35c=40.0)
    draft = rec.run(DRAFT, p)['force']
    return cars, single, gearbox, seq, draft, dict(rec.m.calls)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    first = record(args.game_dir)
    second = record(args.game_dir)
    if first[:5] != second[:5]:
        raise RuntimeError('two runs differ')
    cars, single, gearbox, seq, draft, calls = first
    if draft != 1.0:
        raise RuntimeError(f'draft factor with no cars is {draft}')
    note = 'generated by scripts/research/replay-vehicle-engine.py (x87 control word 0x27f).'
    sizes = [
        write_tsv(args.out_dir / 'vehicle-engine-cars.tsv', cars, [
            'V3 cars: carSpecs (0x1c4 bytes, hex) from nfs5.exe 0x49c82f..0x49cb50, default tuning;',
            note]),
        write_tsv(args.out_dir / 'vehicle-engine-step.tsv', single, [
            'V3 single steps: nfs5.exe Car_EngineDriveForce 0x49e440 (inputs, then out_* written',
            'fields; out_force is the returned ST0 as f64); engine_type overrides sim+0x4c;',
            'dmg_engine = Car_DamageEngine 0x4938a0, dmg_gearbox = Car_DamageGearbox 0x493aa0; ' + note]),
        write_tsv(args.out_dir / 'vehicle-engine-gearbox.tsv', gearbox, [
            'V3 gearbox: nfs5.exe Car_AutoGearbox 0x49daf0 called directly; ' + note]),
        write_tsv(args.out_dir / 'vehicle-engine-seq.tsv', seq, [
            'V3 sequences: repeated 0x49e440 calls; between steps d84 -= 1 (if > 0), gear events',
            'set d81/d82/d84 = sim+0x54/d83 (downshift into a forward gear), d7e = d82, throttle',
            'from the script, v += F/32 (F clamped to the grip sum when traction = 1); ' + note]),
    ]
    print('calls:', ', '.join(f'{t:#x} x{n}' for t, n in sorted(calls.items())))
    print(f'{len(single)} step rows, {len(gearbox)} gearbox rows, {len(seq)} sequence rows, '
          f'{sum(sizes)} bytes; two runs identical -> {args.out_dir}')


if __name__ == '__main__':
    main()
