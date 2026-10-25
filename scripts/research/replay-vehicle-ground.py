"""Record V6 fixtures: the original ground contact and attitude.

Executes nfs5.exe in Unicorn (scripts/research/vehicle_harness.py, x87 control
word 0x27f): `Car_GroundContactTick` 0x49bd40 with everything it calls, i.e.
the scheduler-rate query 0x414b10 (run as is), `Car_WheelGroundAndAttitude`
0x499a70 in full (both wheel loops, the ground normal, the pitch/roll tail,
`Car_ClampLocalAngular` 0x49bc80, the one-side lift, `Car_UpdateWorldPoints`
0x49ae30), the height 0x49a900, 0x494480 (plane distance test), the material
height noise 0x499730 (draws from the shared LCG), `Car_AlignmentNoise`
0x493bc0, the CRT asin 0x5a2a10, `Car_BuildGroundFrame` 0x49ba30, the landing
compression clamp 0x49aec0, the event queue 0x40f020 (car+0x110c) and
`Car_ClampAngularVelocityB` 0x49bba0. Workspace notes: docs/re/vehicle.md
section 1 and "V6".

Ground stub (synthetic support; the original support tree is not loaded).
The per-wheel support owner is the object at w+4 (car+0x7bc + i*0xc4); the
body owner is car+8. Three owner methods are replaced:
- 0x474060 (thiscall, owner, (point, reuse), ret 8): picks the plane under
  the point's x, z from the scenario ground. Writes owner+0x28 = plane
  record address (0 if none), owner+0x24 = the plane's flags word (0 if
  none), owner+2 = 0 (normal not cached), owner+9 = changed, owner+0xa =
  (no plane). The reuse flag is ignored (a plane is a pure function of x, z).
- 0x474420 (thiscall, owner, no args): writes the plane normal to owner+0xc
  and returns owner+0xc (no plane: owner+0xc untouched, as 0x474320 does).
- 0x4745f0 (thiscall, owner, (vertex), ret 4): returns the plane's base
  point (vertex 0; every call site asks for vertex 0), or 0x5bb2b0 with no
  plane.
The plane height at a point is then computed by the original code
(0x499a70, 0x494480) from that normal and base point.
- 0x473330 (thiscall, car+8, (pos, 0), ret 8; overhead support > 3.5 above
  the car) returns 0: no roof/bridge over the car.
- 0x496610 (rollover contact) and 0x4998c0 (+0x52c bit 7 path) are not
  recorded: a closed-loop run that reaches them stops with an error, a
  single-call case that reaches one is discarded and drawn again.

Car setup as in V5 (replay-vehicle-step.py): carSpecs from V1 with default
tuning for 356Acoupe16, boxster25, 993coupe436; res+0x14 = 1; automatic box;
no parts. Additionally: +0x7c = 1 (0x49bd40 gate), +0x78 = 0, car+0 = 0,
+0x10b8 = 0 (0x414b10 row 3 index 6: every call, 32 steps/s, 1 step,
dt 1/32); body points +0x8c in the order of 0x49abc0: four wheel ground
points (-+0.75, -0.35, +-1.2) (FL, FR, RL, RR) then four roof corners
(-+0.8, 0.75, +-1.9), count 8; half extents
+0x3a8 = (0.85, 0.65, 2.1); body surface car+0x2c = 1; the alternate plane
+0x3e8/+0x400 = the start plane; +0x41c = the start plane height under the
car.

Files (vehicle-ground-*.tsv):
- runs: per closed-loop run the scenario, ground and initial state;
- body: state after each 64 Hz step (as V5);
- dyn: state after each 32 Hz step (input word, the V5 dyn columns, and the
  ground/attitude fields GROUND_FIELDS);
- call: single 0x49bd40 calls on random states (in_* before, out_* after).

    python scripts/research/replay-vehicle-ground.py --game-dir local/game \
        --out-dir local/fixtures
"""
import argparse
import importlib.util
import math
from pathlib import Path
import random
import struct
import sys

from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_ESP

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vehicle_harness as vh  # noqa: E402

_spec = importlib.util.spec_from_file_location('vstep', HERE / 'replay-vehicle-step.py')
vs = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(vs)

GROUND_TICK = 0x49bd40
OWNER_SELECT, OWNER_NORMAL, OWNER_VERTEX = 0x474060, 0x474420, 0x4745f0
TRACK_QUERY, ROLLOVER, FLAG80_PATH = 0x473330, 0x496610, 0x4998c0
DEFAULT_VERTEX = 0x5bb2b0
PLANES = vh.SCRATCH + 0x1000      # plane records: normal (12), base (12), flags
PLANE_SIZE = 0x20

# Body point order of 0x49abc0 (the model's points sorted by y, then the four
# lowest by quadrant): 0 = (-x, +z), 1 = (+x, +z), 2 = (-x, -z), 3 = (+x, -z),
# i.e. front left, front right, rear left, rear right (x along R, z along F).
# With this order the contact normal of 0x499a70 points up; the opposite
# x order gives a downward normal and the attitude block is skipped.
WHEEL_POINTS = [(-0.75, -0.35, 1.2), (0.75, -0.35, 1.2), (-0.75, -0.35, -1.2), (0.75, -0.35, -1.2)]
ROOF_POINTS = [(-0.8, 0.75, 1.9), (0.8, 0.75, 1.9), (-0.8, 0.75, -1.9), (0.8, 0.75, -1.9)]
BODY_POINTS = WHEEL_POINTS + ROOF_POINTS
HALF_EXTENTS = (0.85, 0.65, 2.1)
START_XZ = (10.0, 20.0)
SCHED0 = vs.SCHED0
f32, word, short = vs.f32, vs.word, vs.short


def norm(v):
    n = math.sqrt(sum(c * c for c in v))
    return tuple(f32(c / n) for c in v)


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


# -- synthetic ground ------------------------------------------------------
class Plane:
    def __init__(self, normal, base, flags=1):
        self.normal = tuple(f32(c) for c in normal)
        self.base = tuple(f32(c) for c in base)
        self.flags = flags

    def height(self, x, z):
        n, b = self.normal, self.base
        return (-((x - b[0]) * n[0]) - (z - b[2]) * n[2]) / n[1] + b[1]


class Ground:
    """Planes with x/z regions, first match wins; `None` regions are holes
    (no polygon). Each region: (plane or None, xmin, xmax, zmin, zmax)."""

    def __init__(self, name, regions, default):
        self.name, self.regions, self.default = name, regions, default

    def pick(self, x, z):
        for plane, x0, x1, z0, z1 in self.regions:
            if x0 <= x < x1 and z0 <= z < z1:
                return plane
        return self.default

    def planes(self):
        out = [p for p, *_ in self.regions if p is not None]
        if self.default is not None:
            out.append(self.default)
        return list(dict.fromkeys(out))

    def describe(self):
        def p(pl):
            if pl is None:
                return 'none'
            return ','.join(short(c) for c in pl.normal + pl.base) + f',{pl.flags}'
        regs = ';'.join(f'{p(pl)}@{short(x0)},{short(x1)},{short(z0)},{short(z1)}'
                        for pl, x0, x1, z0, z1 in self.regions)
        return f'{p(self.default)}|{regs}'


INF = 1e9
FLAT = Plane((0.0, 1.0, 0.0), (0.0, 0.0, 0.0))


def flat(flags=1, y=0.0):
    return Ground('flat', [], Plane((0.0, 1.0, 0.0), (0.0, y, 0.0), flags))


def slope(gx, gz):
    """y = gx*x + gz*z."""
    return Ground('slope', [], Plane(norm((-gx, 1.0, -gz)), (0.0, 0.0, 0.0)))


def step(height, x0, z0):
    up = Plane((0.0, 1.0, 0.0), (x0, height, z0))
    return Ground('step', [(up, x0, INF, z0, INF)], FLAT)


def bump(height, x0, z0, z1):
    up = Plane((0.0, 1.0, 0.0), (x0, height, z0))
    return Ground('bump', [(up, x0, INF, z0, z1)], FLAT)


# -- recorder --------------------------------------------------------------
def thiscall(nargs, fn):
    def stub(m):
        sp = m.uc.reg_read(UC_X86_REG_ESP)
        if nargs:
            m.put_u32(sp + 4 * nargs, m.get_u32(sp))
            m.uc.reg_write(UC_X86_REG_ESP, sp + 4 * nargs)
        return fn(m, m.uc.reg_read(UC_X86_REG_ECX), sp)
    return stub


# Fields after a ground tick: (column, car offset, kind, count). Kinds: f
# f32, i i32, h u16, b u8. Wheel fields are per wheel (offset from w).
CAR_FIELDS = [
    ('pos_330', 0x330, 'f', 3), ('vel_33c', 0x33c, 'f', 3), ('dir_348', 0x348, 'f', 3),
    ('speed_35c', 0x35c, 'f', 1), ('axes_364', 0x364, 'f', 9), ('ang_388', 0x388, 'f', 3),
    ('side_39c', 0x39c, 'f', 1), ('up_3a0', 0x3a0, 'f', 1), ('fwd_3a4', 0x3a4, 'f', 1),
    ('ay_40c', 0x40c, 'f', 1), ('flags_418', 0x418, 'h', 1), ('mean_41c', 0x41c, 'f', 1),
    ('vy_420', 0x420, 'f', 1), ('height_424', 0x424, 'f', 1), ('height_428', 0x428, 'f', 1),
    ('frame_490', 0x490, 'f', 9), ('wg_d64', 0xd64, 'f', 3), ('pitch_dc4', 0xdc4, 'f', 1),
    ('roll_dc8', 0xdc8, 'f', 1), ('sched_88', 0x88, 'i', 1), ('landing_ad0', 0xad0, 'f', 1),
    ('start_db0', 0xdb0, 'i', 1), ('flip_504', 0x504, 'f', 1), ('events_110c', 0x110c, 'i', 1),
    ('event_1110', 0x1110, 'i', 42), ('world_1dc', 0x1dc, 'f', 24),
]
# Not in the closed-loop rows (size): the event entries (the count is) and the
# world points (they follow from the pose, compared in the body rows).
LOOP_SKIP = ('event_1110', 'world_1dc')
# Inputs that the tick reads but does not write (single-call cases).
INPUT_FIELDS = [
    ('acc_d40', 0xd40, 'f', 3), ('sums_d4c', 0xd4c, 'f', 3), ('gas_d7f', 0xd7f, 'b', 1),
    ('field_84', 0x84, 'f', 1), ('flags_52c', 0x52c, 'i', 1), ('half_3a8', 0x3a8, 'f', 3),
    ('alt_n_3e8', 0x3e8, 'f', 3), ('alt_b_400', 0x400, 'f', 3), ('body_surf_2c', 0x2c, 'h', 1),
    ('remote_dac', 0xdac, 'i', 1),
]
WHEEL_FIELDS = [
    ('query_34', 0x34, 'f', 3), ('point_40', 0x40, 'f', 3), ('normal_4c', 0x4c, 'f', 3),
    ('disp_58', 0x58, 'f', 3), ('gap_64', 0x64, 'f', 1), ('comp_68', 0x68, 'f', 1),
    ('load_6c', 0x6c, 'f', 1), ('angle_70', 0x70, 'f', 1), ('noise_74', 0x74, 'f', 3),
    ('air_84', 0x84, 'i', 1), ('spin_88', 0x88, 'f', 1), ('surf_8c', 0x8c, 'i', 1),
]
FMT = {'f': ('<f', 4), 'i': ('<i', 4), 'h': ('<H', 2), 'b': ('<B', 1)}


class GroundRecorder(vs.Recorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        self.terrain = flat()
        self.addr = {}
        m = self.m
        m.stubs[OWNER_SELECT] = thiscall(2, self._select)
        m.stubs[OWNER_NORMAL] = thiscall(0, self._normal)
        m.stubs[OWNER_VERTEX] = thiscall(1, self._vertex)
        m.stubs[TRACK_QUERY] = thiscall(2, lambda *_: 0)
        self.reached = set()
        m.stubs[ROLLOVER] = self._unreachable(ROLLOVER)
        m.stubs[FLAG80_PATH] = self._unreachable(FLAG80_PATH)
        self.allowed |= set(range(PLANES, PLANES + 0x800))

    def _unreachable(self, va):
        def stub(_m):
            self.reached.add(va)
            return 0
        return stub

    def tick(self):
        self.reached = set()
        self.m.call(GROUND_TICK, vh.CAR)
        return not self.reached

    # -- owner stubs ----------------------------------------------------------
    def set_ground(self, ground):
        self.terrain = ground
        self.addr = {}
        for k, p in enumerate(ground.planes()):
            a = PLANES + k * PLANE_SIZE
            self.m.put(a, *p.normal, *p.base)
            self.m.put_u32(a + 24, p.flags)
            self.addr[id(p)] = a

    def _select(self, m, owner, sp):
        x, _, z = m.get(m.get_u32(sp + 4), 3)
        plane = self.terrain.pick(x, z)
        old = m.get_u32(owner + 0x28)
        new = 0 if plane is None else self.addr[id(plane)]
        m.put_u32(owner + 0x28, new)
        m.put_bytes(owner + 2, [0])
        m.put_bytes(owner + 9, [int(new != old)])
        m.put_bytes(owner + 0xa, [int(new == 0)])
        m.uc.mem_write(owner + 0x24, struct.pack('<H', 0 if plane is None else plane.flags))
        return 0

    def _normal(self, m, owner, _sp):
        poly = m.get_u32(owner + 0x28)
        if poly:
            m.put(owner + 0xc, *m.get(poly, 3))
        return owner + 0xc

    def _vertex(self, m, owner, _sp):
        poly = m.get_u32(owner + 0x28)
        return poly + 12 if poly else DEFAULT_VERTEX

    # -- field dumps ------------------------------------------------------
    def read(self, base, fields, prefix):
        row = []
        for name, off, kind, n in fields:
            fmt, size = FMT[kind]
            vals = [struct.unpack(fmt, self.m.get_bytes(base + off + size * k, size))[0]
                    for k in range(n)]
            row += [(f'{prefix}{name}', vals[0])] if n == 1 else \
                [(f'{prefix}{name}_{k}', v) for k, v in enumerate(vals)]
        return row

    def write(self, base, fields, values):
        for name, off, kind, n in fields:
            fmt, size = FMT[kind]
            for k in range(n):
                key = name if n == 1 else f'{name}_{k}'
                if key in values:
                    self.m.put_bytes(base + off + size * k, struct.pack(fmt, values[key]))

    def ground_row(self, prefix='', full=True):
        fields = CAR_FIELDS if full else [f for f in CAR_FIELDS if f[0] not in LOOP_SKIP]
        row = self.read(vh.CAR, fields, prefix)
        for k in range(4):
            row += self.read(self.m.wheel(k), WHEEL_FIELDS, f'{prefix}w{k}_')
        row += [(f'{prefix}rand_state', self.m.get_u32(vs.G_RAND_STATE)),
                (f'{prefix}rand_last', self.m.get_u32(vs.G_RAND_LAST))]
        return row

    # -- closed-loop setup ----------------------------------------------------
    def place(self, ground, lift, slope_axes):
        """Pose: axes aligned with the start plane (heading +z) if
        `slope_axes`, else world axes; position such that the wheel points
        lie on the plane, plus `lift` along y."""
        p = ground.pick(*START_XZ) or FLAT
        if slope_axes:
            n = p.normal
            r = norm(cross(n, (0.0, 0.0, 1.0)))
            f = norm(cross(r, n))
            axes = (r, n, f)
        else:
            axes = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))
        x, z = START_XZ
        y = p.height(x, z)
        u = axes[1]
        pos = tuple(f32(c) for c in (x + 0.35 * u[0], y + 0.35 * u[1] + lift, z + 0.35 * u[2]))
        return axes, pos, f32(y)

    def setup_run(self, car, v0, wy0, mul, ground, lift=0.0, slope_axes=False):
        gear = self.setup(car, v0, wy0, mul)
        m = self.m
        self.set_ground(ground)
        axes, pos, gy = self.place(ground, lift, slope_axes)
        m.car_put(0x364, *[c for a in axes for c in a])
        m.car_put(0x330, *pos)
        f = axes[2]
        vel = tuple(f32(v0 * c) for c in f)
        m.car_put(0x33c, *vel)
        m.car_put(0x35c, vs.octagonal(vel[0], vel[2]))
        m.put_i32(vh.CAR + 0x32c, len(BODY_POINTS))
        for k, pt in enumerate(BODY_POINTS):
            m.car_put(0x8c + 12 * k, *pt)
        m.car_put(0x3a8, *HALF_EXTENTS)
        m.put_bytes(vh.CAR + 0x7c, [1])
        m.uc.mem_write(vh.CAR + 0x2c, struct.pack('<H', 1))
        start = ground.pick(*START_XZ)
        m.car_put(0x3e8, *start.normal)
        m.car_put(0x400, *start.base)
        m.car_put(0x41c, gy)
        # World points and the wheel points w+0x40 from the pose (0x49ae30).
        rows = list(zip(*axes))
        for k, pt in enumerate(BODY_POINTS):
            w = [f32(sum(pt[j] * rows[i][j] for j in range(3)) + pos[i]) for i in range(3)]
            if k < 4:
                m.put(m.wheel(k) + 0x40, *w)
        m.call(0x49ae30, vh.CAR)
        nrm = start.normal
        m.put(vs.NORMAL, *nrm)
        up = f32(sum(axes[1][i] * nrm[i] for i in range(3)))
        m.car_put(0x3a0, up)
        m.call(vs.GROUND_FRAME, vh.CAR, vs.NORMAL, struct.unpack('<I', struct.pack('<f', up))[0])
        return gear

    def run_v6(self, car, name, v0, wy0, steps, script, ground, mul=1, lift=0.0, slope_axes=False):
        m = self.m
        run = f'{car}/{name}'
        gear = self.setup_run(car, v0, wy0, mul, ground, lift, slope_axes)
        init = [('run', run), ('car', car), ('v0', f32(v0)), ('wy0', f32(wy0)), ('gear0', gear),
                ('rpm0', self.f(0xdb4)), ('steps', steps), ('sched0', SCHED0),
                ('rand_state0', m.get_u32(vs.G_RAND_STATE)), ('rand_mul', mul),
                ('ground', ground.describe())]
        init += self.ground_row('init_')
        body, dyn = [], []
        self.written = set()
        for n in range(steps):
            s = SCHED0 + n
            m.put_u32(vs.G_SCHED, s)
            if s % 2 == 0:
                w = script(n // 2)
                m.put_u32(vs.G_INPUT_WORD, w)
                m.call(vs.READ_INPUTS, vh.CAR)
                m.call(vs.DYNAMICS_TICK, vh.CAR)
                if not self.tick():
                    raise RuntimeError(f'{run} step {n}: reached {sorted(map(hex, self.reached))}')
                dyn.append(self.dyn_row(run, n, w) + self.ground_row(full=False))
            m.call(vs.INTEGRATE_POS, vh.CAR)
            m.call(vs.INTEGRATE_ORI, vh.CAR)
            body.append(self.body_row(run, n))
        bad = self.written - self.allowed
        if bad:
            raise RuntimeError(f'{run}: unexpected writes {sorted(hex(a) for a in bad)[:20]}')
        return init, body, dyn

    # -- single calls -----------------------------------------------------
    def single(self, k, rng):
        """One 0x49bd40 call on a random state."""
        m = self.m
        car = rng.choice([c for c, _, _ in vs.CARS])
        self.setup(car, 0.0, 0.0, rng.choice((1, 0x343fd)))
        kind = rng.choice(('flat', 'flat', 'slope', 'step', 'hole', 'far', 'gravel'))
        if kind == 'flat':
            g = flat(rng.choice((1, 1, 2, 4, 7, 10)))
        elif kind == 'slope':
            g = slope(f32(rng.uniform(-0.15, 0.15)), f32(rng.uniform(-0.15, 0.15)))
        elif kind == 'step':
            g = step(f32(rng.uniform(-0.2, 0.2)), f32(rng.uniform(9.0, 11.0)),
                     f32(rng.uniform(18.5, 21.5)))
        elif kind == 'hole':
            g = Ground('hole', [(None, f32(rng.uniform(9.0, 11.0)), INF, -INF, INF)], FLAT)
        elif kind == 'far':
            g = Ground('far', [(Plane((0.0, 1.0, 0.0), (0.0, -5.0, 0.0)),
                                f32(rng.uniform(9.0, 11.0)), INF, -INF, INF)], FLAT)
        else:
            g = flat(rng.choice((3, 5, 9)))
        self.set_ground(g)
        # Random pose near the plane: small tilt, height offset.
        lift = rng.choice((0.0, 0.0, f32(rng.uniform(-0.15, 0.15)), f32(rng.uniform(0.0, 2.0))))
        axes, pos, gy = self.place(g, lift, rng.random() < 0.5)
        tilt = [f32(rng.uniform(-0.12, 0.12)) for _ in range(2)]
        u = norm((axes[1][0] + tilt[0], axes[1][1], axes[1][2] + tilt[1]))
        r = norm(cross(u, axes[2]))
        f = norm(cross(r, u))
        if rng.random() < 0.08:          # steep: attitude gate 0.707 / normal < 0.5
            pos = (pos[0], f32(pos[1] + rng.uniform(1.5, 3.0)), pos[2])
            u = norm((rng.uniform(0.7, 1.4), 1.0, 0.0))
            r = norm(cross(u, (0.0, 0.0, 1.0)))
            f = norm(cross(r, u))
        axes = (r, u, f)
        m.car_put(0x364, *[c for a in axes for c in a])
        m.car_put(0x330, *pos)
        sp = f32(rng.choice((0.0, rng.uniform(0, 1), rng.uniform(0, 45))))
        vel = [f32(sp * c) for c in f]
        vel[1] = f32(rng.choice((0.0, rng.uniform(-9, 3), rng.uniform(-1, 1))))
        if rng.random() < 0.1:
            vel = [0.0, 0.0, 0.0]
        m.car_put(0x33c, *vel)
        m.car_put(0x388, *[f32(rng.choice((0.0, rng.uniform(-0.6, 0.6)))) for _ in range(3)])
        m.put_i32(vh.CAR + 0x32c, len(BODY_POINTS))
        for j, pt in enumerate(BODY_POINTS):
            m.car_put(0x8c + 12 * j, *pt)
        m.call(0x49ae30, vh.CAR)
        m.car_put(0x3a8, *HALF_EXTENTS)
        m.put_bytes(vh.CAR + 0x7c, [1])
        values = {
            'acc_d40_0': f32(rng.uniform(-12, 12)), 'acc_d40_1': f32(rng.uniform(-3, 3)),
            'acc_d40_2': f32(rng.uniform(-12, 12)),
            'sums_d4c_0': f32(rng.uniform(-14, 14)), 'sums_d4c_2': f32(rng.uniform(-14, 14)),
            'gas_d7f': rng.choice((0, 0, 128, 254)), 'field_84': f32(rng.choice((1.16, 1.16, 1.3))),
            'flags_52c': rng.choice((0, 0, 0, 0, 0x20, 0x400)),
            'body_surf_2c': rng.choice((1, 2, 7, 12)), 'remote_dac': rng.choice((0,) * 9 + (1,)),
            'speed_35c': f32(rng.uniform(0, 45)), 'up_3a0': f32(rng.choice((1.0, 0.95, 0.9, 0.8, 0.6))),
            'pitch_dc4': f32(rng.uniform(-0.05, 0.05)), 'roll_dc8': f32(rng.uniform(-0.05, 0.05)),
            'mean_41c': f32(gy + rng.choice((0.0, rng.uniform(-0.1, 0.1)))),
            'vy_420': f32(rng.choice((0.0, rng.uniform(-2, 2)))), 'ay_40c': f32(rng.uniform(-1, 1)),
            'height_428': f32(rng.choice((0.0, 0.01))),
        }
        alt = rng.choice((g.pick(*START_XZ) or FLAT, Plane(norm((0.1, 1.0, 0.0)), (0, 0.3, 0)),
                          Plane((0.0, 0.05, 1.0), (0, 0, 0))))
        for j in range(3):
            values[f'alt_n_3e8_{j}'] = alt.normal[j]
            values[f'alt_b_400_{j}'] = alt.base[j]
        for i in range(3):
            values[f'half_3a8_{i}'] = HALF_EXTENTS[i]
        self.write(vh.CAR, CAR_FIELDS + INPUT_FIELDS, values)
        for w in range(4):
            wv = {'comp_68': f32(rng.uniform(-0.2, 0.1)), 'air_84': rng.choice((0, 0, 1)),
                  'spin_88': f32(rng.random()), 'noise_74_0': f32(rng.uniform(-0.02, 0.02)),
                  'noise_74_1': f32(rng.uniform(-0.02, 0.02)), 'noise_74_2': f32(rng.uniform(0, 3))}
            self.write(m.wheel(w), WHEEL_FIELDS, wv)
        handling = rng.choice((0, 0, 0, 1))
        res518 = rng.choice((0,) * 7 + (1,))
        sched = rng.choice((SCHED0, SCHED0 + 2, 0x2000))
        m.put_u32(0x6573f8, handling)
        m.put_i32(vh.RES + 0x518, res518)
        m.put_u32(vs.G_SCHED, sched)
        head = [('case', k), ('car', car), ('ground', g.describe()), ('handling', handling),
                ('res_518', res518), ('sched', sched), ('rand_mul', m.get_u32(vs.G_RAND_MUL))]
        before = self.ground_row('in_') + self.read(vh.CAR, INPUT_FIELDS, 'in_')
        self.written = set()
        if not self.tick():
            return None
        bad = self.written - self.allowed
        if bad:
            raise RuntimeError(f'case {k}: unexpected writes {sorted(hex(a) for a in bad)[:20]}')
        return head + before + self.ground_row('out_')


def scenarios():
    const = lambda w: (lambda k: w)  # noqa: E731
    return [
        # (name, v0, wy0, steps, script, ground, mul, lift, slope_axes)
        ('brake_dive', 30.0, 0.0, 256, const(word(brake=254)), flat(), 1, 0.0, False),
        ('accel_squat', 0.0, 0.0, 320, const(word(gas=254)), flat(), 1, 0.0, False),
        ('turn_roll', 20.0, 0.0, 320, const(word(gas=110, steer=100)), flat(), 1, 0.0, False),
        ('step', 15.0, 0.0, 192, const(word(gas=60)), step(0.06, 10.3, 30.0), 1, 0.0, False),
        ('bump', 15.0, 0.0, 192, const(word(gas=60)), bump(0.04, 10.3, 28.0, 28.4), 1, 0.0, False),
        ('slope_up', 20.0, 0.0, 256, const(word(gas=200)), slope(0.0, 0.05), 1, 0.0, True),
        ('camber', 20.0, 0.0, 256, const(word(gas=80)), slope(0.06, 0.0), 1, 0.0, True),
        ('drop_small', 10.0, 0.0, 192, const(word()), flat(), 1, 0.4, False),
        ('drop_hard', 15.0, 0.0, 256, const(word()), flat(), 1, 2.5, False),
        ('gravel', 20.0, 0.0, 192, const(word(gas=100)), flat(3), 0x343fd, 0.0, False),
    ]


def record(game_dir, cases):
    rec = GroundRecorder(game_dir)
    runs, body, dyn = [], [], []
    for car, _, _ in vs.CARS:
        for name, v0, wy0, steps, script, ground, mul, lift, sa in scenarios():
            r, b, d = rec.run_v6(car, name, v0, wy0, steps, script, ground, mul, lift, sa)
            runs.append(r)
            body += b
            dyn += d
    rng = random.Random(0x49bd40)
    calls = []
    while len(calls) < cases:
        row = rec.single(len(calls), rng)
        if row is not None:
            calls.append(row)
    return runs, body, dyn, calls, dict(rec.m.calls)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--cases', type=int, default=400)
    args = parser.parse_args()
    first = record(args.game_dir, args.cases)
    second = record(args.game_dir, args.cases)
    if first[:4] != second[:4]:
        raise RuntimeError('two runs differ')
    runs, body, dyn, calls, ncalls = first
    note = 'generated by scripts/research/replay-vehicle-ground.py (x87 control word 0x27f).'
    out = args.out_dir
    sizes = [
        vs.write_tsv(out / 'vehicle-ground-runs.tsv', runs, [
            'V6 runs: scenario, ground (default plane|regions: normal,base,flags@x0,x1,z0,z1) and the',
            'initial state (init_*); ' + note]),
        vs.write_tsv(out / 'vehicle-ground-body.tsv', body, [
            'V6 body: state after each 64 Hz step (0x49b7a0, 0x49b8e0; on even steps after 0x4991e0,',
            '0x49fd30 and Car_GroundContactTick 0x49bd40); ' + note]),
        vs.write_tsv(out / 'vehicle-ground-dyn.tsv', dyn, [
            'V6 dyn: input word, V5 dyn columns and the ground/attitude fields after 0x4991e0, 0x49fd30',
            'and 0x49bd40 on each even (32 Hz) step; ' + note]),
        vs.write_tsv(out / 'vehicle-ground-call.tsv', calls, [
            'V6 single Car_GroundContactTick 0x49bd40 calls: in_* state before, out_* after; ' + note]),
    ]
    print('calls:', ', '.join(f'{t:#x} x{n}' for t, n in sorted(ncalls.items())))
    print(f'{len(runs)} runs, {len(body)} body rows, {len(dyn)} dyn rows, {len(calls)} call rows, '
          f'{sum(sizes)} bytes; two runs identical -> {out}')


if __name__ == '__main__':
    main()
