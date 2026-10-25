"""Record V5 fixtures: closed-loop runs of the original planar step and the
64 Hz integration on a flat plane.

Executes nfs5.exe in Unicorn (scripts/research/vehicle_harness.py, x87 control
word 0x27f). Each 64 Hz scheduler step s (g_SchedStep [0x606ac4] = s) is, in
the order of Sched_Step 0x414ff0 and the queues in docs/re/vehicle.md
section 1 (workspace root):

- even s (32 Hz queue): g_InputWord [0x5e9664] = the script's word, then
  Car_ReadInputs 0x4991e0 (prio 0x15), Car_DynamicsTick 0x49fd30 (prio 0x1e;
  calls Car_PlanarDynamics 0x49f160 -> Car_ApplyControls 0x49dd20,
  Car_AlignmentNoise 0x493bc0, Car_EngineDriveForce 0x49e440,
  Car_DamageBrakes 0x493a10, Car_WheelGripLimits 0x4a1900, Wheel_TyreForce
  0x4a20d0 x4, Car_SlopeGravity 0x49f0c0), then the flat-ground stand-in for
  Car_GroundContactTick 0x49bd40 (prio 0x1e, after the dynamics):
  Car_BuildGroundFrame 0x49ba30 (car, normal (0,1,0), +0x3a0) and the speed
  +0x35c = max(|v.x|,|v.z|) + 0.25 min(...) (the formula of 0x49bd40
  [VA 0x49bdd0..]); +0x3a0 = 1, +0x424 = 0 held, wheels w+0x84 = 0 (on the
  ground), w+0x8c = 1;
- every s (64 Hz queue): Car_IntegratePosition 0x49b7a0 (pos and wheel points
  w+0x40 += vel/64, Car_WheelEffects 0x4a2880, Car_SpinWheels 0x4a2790), then
  Car_IntegrateOrientation 0x49b8e0 (axes = axes * (Rx*Ry)*Rz of omega/64,
  Car_UpdateWorldPoints 0x49ae30 for 4 body points, every 32 calls
  0x4a4810);
- then s += 1.

Not run: Car_TrackLocate, Car_TrackProgressTick, Car_CollideTick, the
contact effects, and the real Car_GroundContactTick (pitch/roll, vertical
velocity; V6). The effect emitter 0x478f30 called by 0x4a2880 (thiscall on
the wheel's effect object, 7 args, `ret 0x1c`; rendering/sound only) runs
with its enable byte [0x6197e7] = 0, so it returns at once. Stubs (no-ops),
as in V4: the AI planners 0x40b650 / 0x40bfe0, the remote lock pair
0x5322b0 / 0x5322c0 and the +0xe10 callback (none of them is reached on the
player path).

Car setup: carSpecs from the V1 derivation (default tuning) for 356Acoupe16,
boxster25 and 993coupe436; res+0x14 = 1 (steering rate limit), other res
flags 0; no parts, no damage; +0x7f = 1 (active), +0x7d = 0, reortho counter
+0x80 = 0x20; race started (db0 = 0x100, s from 0x200, automatic box); axes
R = +x, U = +y, F = +z; ground frame from 0x49ba30; 4 body points and the
wheel points at fixed offsets. The LCG starts from the shipped state and
multiplier (one extra Boxster run uses multiplier 0x343fd).

Files (vehicle-step-*.tsv):
- cars: carSpecs bytes (hex) per car;
- runs: per run the initial state and inputs-independent setup, and the
  final state after the last step (wheel/world points, accels d34..d48,
  force feedback, LCG, counters);
- body: per 64 Hz step the state after it: position, velocity, angular
  velocity (turns/s), axes, wheel spin phases w+0x88;
- dyn: per 32 Hz step (even s) the input word and the state after the
  dynamics and the ground stand-in: d4c/d54, d58/d5c/d60, d64..d6c, dc0,
  +0x35c, engine and gear fields, controls, globals, and per wheel the tyre
  outputs.

Two runs of the recorder are compared (determinism). Every write outside the
stack goes to the car struct or to a known global (checked).

    python scripts/research/replay-vehicle-step.py --game-dir local/game \
        --out-dir local/fixtures
"""
import argparse
import math
from pathlib import Path
import struct
import sys

from unicorn import UC_HOOK_MEM_WRITE

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vehicle_harness as vh  # noqa: E402

READ_INPUTS, DYNAMICS_TICK = 0x4991e0, 0x49fd30
GROUND_FRAME, INTEGRATE_POS, INTEGRATE_ORI = 0x49ba30, 0x49b7a0, 0x49b8e0
G_EFFECTS_ON = 0x6197e7     # byte; 0 makes 0x478f30 return at once
STUBS_NOP = (0x40b650, 0x40bfe0, 0x5322b0, 0x5322c0)
CALLBACK_STUB = vh.EXIT + 0x100
G_INPUT_WORD, G_NET_PLAYERS, G_MIRROR = 0x5e9664, 0x5e8e28, 0x65742c
G_THROTTLE, G_BRAKE, G_STEER_ABS = 0x628fb0, 0x628fac, 0x628fb4
G_SCHED, G_RAND_STATE, G_RAND_MUL, G_RAND_LAST = 0x606ac4, 0x5d1028, 0x5d102c, 0x655a0c
NORMAL = vh.SCRATCH          # ground normal for 0x49ba30

# Globals the step may write (address, size).
GLOBAL_WRITES = [(0x628f98, 0x20), (G_RAND_STATE, 4), (G_RAND_LAST, 4), (G_SCHED, 4)]

CARS = [('356A', '356Acoupe16.sim', '356A'), ('Boxster', 'boxster25.sim', 'Boxster'),
        ('993C4', '993coupe436.sim', '993')]

WHEEL_POINTS = [(0.75, 0.0, 1.2), (-0.75, 0.0, 1.2), (0.75, 0.0, -1.2), (-0.75, 0.0, -1.2)]
BODY_POINTS = [(0.9, 0.5, 2.1), (-0.9, 0.5, 2.1), (0.9, 0.5, -2.1), (-0.9, 0.5, -2.1)]
START_POS = (10.0, 0.0, 20.0)
SURFACE = 1
SCHED0 = 0x200


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def short(x):
    """Shortest decimal that parses back to the same f32 (Rust `parse::<f32>`)."""
    x = f32(x)
    if math.isnan(x):
        return 'NaN'
    for p in range(1, 10):
        s = '%.*g' % (p, x)
        if f32(float(s)) == x and math.copysign(1, float(s)) == math.copysign(1, x):
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


def word(gas=0, brake=0, steer=0, handbrake=0):
    """Input word as Input_PollControllers packs it (as in the V4 recorder):
    byte0 bit0 handbrake, byte1 steer/2 (signed), byte2 gas/2, byte3 brake/2."""
    return ((handbrake & 1) | ((steer // 2) & 0xff) << 8 | ((gas // 2) & 0xff) << 16
            | ((brake // 2) & 0xff) << 24)


def octagonal(vx, vz):
    """+0x35c of Car_GroundContactTick 0x49bd40: max + 0.25 min of |v.x|, |v.z|."""
    a, b = abs(vx), abs(vz)
    return f32(b * 0.25 + a) if a > b else f32(a * 0.25 + b)


# Scenarios: (name, initial speed m/s, initial yaw rate turns/s, 64 Hz steps,
# script(k) -> input word for the k-th 32 Hz step).
def scenarios():
    def const(w):
        return lambda k: w
    return [
        ('accel', 0.0, 0.0, 512, const(word(gas=254))),
        ('brake', 35.0, 0.0, 256, const(word(brake=254))),
        ('turn', 20.0, 0.0, 320, const(word(gas=110, steer=100))),
        ('handbrake', 30.0, 0.0, 192, const(word(steer=40, handbrake=1))),
        ('handbrake_off', 30.0, 0.0, 128, const(word(steer=40))),
        ('liftoff', 25.0, 0.0, 256, lambda k: word(gas=254 if k < 64 else 0, steer=120)),
        ('yaw_recovery', 25.0, 0.5, 192, const(word())),
        ('slide_recover', 30.0, 0.0, 448,
         lambda k: word(steer=60, handbrake=1) if k < 32 else word(gas=254)),
    ]


class Recorder:
    def __init__(self, game_dir):
        module, data = vh.load_module('nfs5.exe', game_dir)
        stubs = {va: (lambda m: 0) for va in STUBS_NOP}
        stubs[CALLBACK_STUB] = lambda m: 0
        self.m = vh.Machine(module, data, stubs=stubs, trace_calls=True)
        self.written = set()
        stack = (vh.STACK_TOP - 0x10000, vh.STACK_TOP)

        def on_write(_uc, _access, address, size, _value, _user):
            if not stack[0] <= address < stack[1]:
                self.written.update(range(address, address + size))

        self.m.uc.hook_add(UC_HOOK_MEM_WRITE, on_write)
        self.specs = {}
        for name, sim, model in CARS:
            raw = (game_dir / 'GameData/Simulation/CarData' / sim).read_bytes()
            self.specs[name] = vh.derive_specs(self.m, raw, model)
        self.fields = {name: dict(vh.unpack_fields(self.specs[name], vh.CARSPECS_FIELDS))
                       for name, _, _ in CARS}
        self.allowed = set(range(vh.CAR, vh.CAR + vh.CAR_SIZE))
        for addr, size in GLOBAL_WRITES:
            self.allowed |= set(range(addr, addr + size))

    # -- memory shorthands --------------------------------------------------
    def f(self, off, n=1):
        v = self.m.car_get(off, n)
        return v[0] if n == 1 else v

    def i(self, off):
        return self.m.get_i32(vh.CAR + off)[0]

    def b(self, off, signed=False):
        return struct.unpack('<b' if signed else '<B', self.m.get_bytes(vh.CAR + off, 1))[0]

    def g(self, addr):
        return self.m.get(addr)[0]

    def setup(self, car, v0, wy0, mul):
        m, fs = self.m, self.fields[car]
        m.reset_car()
        m.set_globals(g_SchedStep=SCHED0, g_RandMul=mul)
        m.put_bytes(vh.SPECS, self.specs[car])
        m.put_i32(vh.RES + 0x14, 1)
        m.put_i32(G_NET_PLAYERS, 0)
        m.put_i32(G_MIRROR, 0)
        m.put_bytes(G_EFFECTS_ON, [0])
        for addr in (G_THROTTLE, G_BRAKE, G_STEER_ABS, G_RAND_LAST):
            m.put_u32(addr, 0)
        m.put_u32(vh.CAR + 0xe10, CALLBACK_STUB)
        m.put_bytes(vh.CAR + 0x7f, [1])
        m.put_bytes(vh.CAR + 0x80, [0x20])
        m.car_put(0x330, *START_POS)
        m.car_put(0x33c, 0.0, 0.0, v0)
        m.car_put(0x364, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)
        m.car_put(0x388, 0.0, wy0, 0.0)
        m.car_put(0x3a0, 1.0)
        m.car_put(0x35c, octagonal(0.0, f32(v0)))
        m.put_i32(vh.CAR + 0x32c, len(BODY_POINTS))
        for k, p in enumerate(BODY_POINTS):
            m.car_put(0x8c + 12 * k, *p)
        for w in range(4):
            base = m.wheel(w)
            m.put(base + 0x40, *[START_POS[j] + WHEEL_POINTS[w][j] for j in range(3)])
            m.put_i32(base + 0x8c, SURFACE)
            m.put_i32(base + 0xc0, 1 if w < 2 else 0)
        # Gear: the lowest forward gear below 0.9 redline (1st from rest).
        red = fs['redline_94']
        top = fs['gear_count_50'] - 1
        gear = 2
        while gear < top and v0 * fs[f'gear_ratio_60_{gear}'] >= 0.9 * red:
            gear += 1
        for off in (0xd7e, 0xd81, 0xd82):
            m.put_bytes(vh.CAR + off, [gear])
        m.car_put(0xdb4, f32(v0 * fs[f'gear_ratio_60_{gear}']))
        m.put_i32(vh.CAR + 0xdb0, 0x100)
        self.ground()
        return gear

    def ground(self):
        """Flat-ground stand-in for Car_GroundContactTick: frame and speed."""
        m = self.m
        m.put(NORMAL, 0.0, 1.0, 0.0)
        m.call(GROUND_FRAME, vh.CAR, NORMAL, struct.unpack('<I', struct.pack('<f', self.f(0x3a0)))[0])
        vx, _, vz = self.f(0x33c, 3)
        m.car_put(0x35c, octagonal(vx, vz))

    def body_row(self, run, step):
        row = [('run', run), ('step', step)]
        for name, off in (('pos', 0x330), ('vel', 0x33c), ('ang', 0x388)):
            row += [(f'{name}_{c}', v) for c, v in zip('xyz', self.f(off, 3))]
        for name, off in (('r', 0x364), ('u', 0x370), ('f', 0x37c)):
            row += [(f'{name}_{c}', v) for c, v in zip('xyz', self.f(off, 3))]
        row += [(f'spin_88_{w}', self.m.get(self.m.wheel(w) + 0x88)[0]) for w in range(4)]
        return row

    def dyn_row(self, run, step, w):
        row = [('run', run), ('step', step), ('input_word', w)]
        for name, off in (('lat_d4c', 0xd4c), ('long_d54', 0xd54), ('vlat_d58', 0xd58),
                          ('vnorm_d5c', 0xd5c), ('vfwd_d60', 0xd60), ('wp_d64', 0xd64),
                          ('wy_d68', 0xd68), ('wr_d6c', 0xd6c), ('slip_dc0', 0xdc0),
                          ('speed_35c', 0x35c), ('rpm_db4', 0xdb4), ('filt_e04', 0xe04),
                          ('filt_e08', 0xe08), ('steer_ac8', 0xac8), ('steer_acc', 0xacc)):
            row.append((name, self.f(off)))
        for name, off in (('gear_req_d7e', 0xd7e), ('prev_gear_d81', 0xd81),
                          ('gear_d82', 0xd82), ('shift_timer_d84', 0xd84)):
            row.append((name, self.b(off, signed=True)))
        for name, off in (('gas_d7f', 0xd7f), ('brake_d80', 0xd80), ('downshift_d83', 0xd83),
                          ('handbrake_d85', 0xd85)):
            row.append((name, self.b(off)))
        for name, off in (('steer_d94', 0xd94), ('reverse_d98', 0xd98), ('limiter_548', 0x548),
                          ('spin_db8', 0xdb8), ('spin_dbc', 0xdbc)):
            row.append((name, self.i(off)))
        row += [('throttle', self.g(G_THROTTLE)), ('brake', self.g(G_BRAKE))]
        for k in range(4):
            base = self.m.wheel(k)
            for name, off in (('lat_90', 0x90), ('long_98', 0x98), ('demand_a8', 0xa8),
                              ('grip_ac', 0xac), ('steer_b4', 0xb4), ('skid_b8', 0xb8)):
                row.append((f'{name}_{k}', self.m.get(base + off)[0]))
            row.append((f'locked_bc_{k}', self.m.get_i32(base + 0xbc)[0]))
        return row

    def run(self, car, name, v0, wy0, steps, script, mul=1):
        m = self.m
        run = f'{car}/{name}'
        gear = self.setup(car, v0, wy0, mul)
        init = [('run', run), ('car', car), ('v0', f32(v0)), ('wy0', f32(wy0)), ('gear0', gear),
                ('rpm0', self.f(0xdb4)), ('speed0', self.f(0x35c)), ('steps', steps),
                ('sched0', SCHED0), ('rand_state0', m.get_u32(G_RAND_STATE)), ('rand_mul', mul)]
        body, dyn = [], []
        self.written = set()
        for n in range(steps):
            s = SCHED0 + n
            m.put_u32(G_SCHED, s)
            if s % 2 == 0:
                w = script(n // 2)
                m.put_u32(G_INPUT_WORD, w)
                m.call(READ_INPUTS, vh.CAR)
                m.call(DYNAMICS_TICK, vh.CAR)
                self.ground()
                dyn.append(self.dyn_row(run, n, w))
            m.call(INTEGRATE_POS, vh.CAR)
            m.call(INTEGRATE_ORI, vh.CAR)
            body.append(self.body_row(run, n))
        m.put_u32(G_SCHED, SCHED0 + steps)
        bad = self.written - self.allowed
        if bad:
            raise RuntimeError(f'{run}: unexpected writes {sorted(hex(a) for a in bad)[:20]}')
        car_writes = sorted({a - vh.CAR for a in self.written if vh.CAR <= a < vh.CAR + vh.CAR_SIZE})
        final = []
        for k in range(4):
            final += [(f'point_40_{k}_{c}', v) for c, v in zip('xyz', m.get(m.wheel(k) + 0x40, 3))]
        for k in range(len(BODY_POINTS)):
            final += [(f'world_1dc_{k}_{c}', v) for c, v in zip('xyz', self.f(0x1dc + 12 * k, 3))]
        for name, off in (('wacc_d34', 0xd34), ('acc_d40', 0xd40)):
            final += [(f'{name}_{c}', v) for c, v in zip('xyz', self.f(off, 3))]
        final += [(name, self.f(off)) for name, off in (
            ('ff_55c', 0x55c), ('ff_560', 0x560), ('ff_564', 0x564), ('boost_54c', 0x54c),
            ('field_84', 0x84))]
        final += [('counter_80', self.b(0x80)), ('gas_d7c', self.b(0xd7c)),
                  ('brake_d7d', self.b(0xd7d)), ('steer_in_d90', self.i(0xd90)),
                  ('start_db0', self.i(0xdb0)), ('steer_abs', self.g(G_STEER_ABS)),
                  ('front_contact', m.get_i32(0x628f9c)[0]), ('rear_contact', m.get_i32(0x628f98)[0]),
                  ('grav_x', self.g(0x628fa0)), ('grav_y', self.g(0x628fa4)), ('grav_z', self.g(0x628fa8)),
                  ('rand_state', m.get_u32(G_RAND_STATE)), ('rand_last', m.get_u32(G_RAND_LAST))]
        return init + [(f'final_{k}', v) for k, v in final], body, dyn, car_writes


def frame_cases(rec):
    """Single calls of Car_BuildGroundFrame 0x49ba30 (car, normal, up) with
    random axes and normals: the flat copy (up > 0.9, +0x52c bit 0 clear),
    and both cross-product branches."""
    import random
    rng = random.Random(0x49ba30)
    m, rows = rec.m, []

    def unit():
        while True:
            v = [rng.uniform(-1, 1) for _ in range(3)]
            n = math.sqrt(sum(c * c for c in v))
            if 0.2 < n <= 1:
                return [f32(c / n) for c in v]

    for k in range(240):
        m.reset_car()
        axes = unit() + unit() + unit()
        normal = unit()
        up = rng.choice((1.0, 0.95, 0.9, 0.89, 0.6, 0.5, 0.49, 0.3, -0.2, f32(rng.uniform(-1, 1))))
        flags = rng.choice((0, 0, 1))
        m.car_put(0x364, *axes)
        m.put_bytes(vh.CAR + 0x52c, [flags])
        m.put(NORMAL, *normal)
        m.call(GROUND_FRAME, vh.CAR, NORMAL, struct.unpack('<I', struct.pack('<f', f32(up)))[0])
        row = [('case', k), ('flags_52c', flags), ('up', f32(up))]
        row += [(f'axes_{j}', v) for j, v in enumerate(axes)]
        row += [(f'normal_{j}', v) for j, v in enumerate(normal)]
        row += [(f'out_{j}', v) for j, v in enumerate(rec.f(0x490, 9))]
        rows.append(row)
    return rows


def record(game_dir):
    rec = Recorder(game_dir)
    cars = [[('car', name), ('sim', sim), ('model', model), ('specs_hex', rec.specs[name].hex())]
            for name, sim, model in CARS]
    runs, body, dyn, writes = [], [], [], set()
    jobs = [(car, *sc) for car, _, _ in CARS for sc in scenarios()]
    jobs.append(('Boxster', 'accel_lcg', 0.0, 0.0, 512, lambda k: word(gas=254), 0x343fd))
    for job in jobs:
        r, b, d, w = rec.run(*job)
        runs.append(r)
        body += b
        dyn += d
        writes |= set(w)
    frames = frame_cases(rec)
    return cars, runs, body, dyn, frames, sorted(writes), dict(rec.m.calls)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    first = record(args.game_dir)
    second = record(args.game_dir)
    if first[:5] != second[:5]:
        raise RuntimeError('two runs differ')
    cars, runs, body, dyn, frames, writes, calls = first
    note = 'generated by scripts/research/replay-vehicle-step.py (x87 control word 0x27f).'
    out = args.out_dir
    sizes = [
        write_tsv(out / 'vehicle-step-cars.tsv', cars, [
            'V5 cars: carSpecs (0x1c4 bytes, hex) from 0x49c82f..0x49cb50, default tuning;', note]),
        write_tsv(out / 'vehicle-step-runs.tsv', runs, [
            'V5 runs: initial conditions and the final state after the last 64 Hz step; ' + note]),
        write_tsv(out / 'vehicle-step-body.tsv', body, [
            'V5 body: state after each 64 Hz step (0x49b7a0, 0x49b8e0; on even steps after',
            '0x4991e0, 0x49fd30 and the flat-ground stand-in); ' + note]),
        write_tsv(out / 'vehicle-step-dyn.tsv', dyn, [
            'V5 dyn: input word and state after 0x4991e0, 0x49fd30 and the flat-ground stand-in',
            'on each even (32 Hz) step; ' + note]),
        write_tsv(out / 'vehicle-step-frame.tsv', frames, [
            'V5 Car_BuildGroundFrame 0x49ba30 (car, normal, up) single calls: out = +0x490..+0x4b0; ' + note]),
    ]
    spans, start = [], None
    for a in writes + [None]:
        if start is None:
            start = prev = a
        elif a is None or a != prev + 1:
            spans.append(f'{start:#x}' if prev == start else f'{start:#x}..{prev:#x}')
            start = prev = a
        else:
            prev = a
    print('car bytes written:', ', '.join(spans))
    print('calls:', ', '.join(f'{t:#x} x{n}' for t, n in sorted(calls.items())))
    print(f'{len(runs)} runs, {len(body)} body rows, {len(dyn)} dyn rows, {len(frames)} frame rows, '
          f'{sum(sizes)} bytes; two runs identical -> {out}')


if __name__ == '__main__':
    main()
