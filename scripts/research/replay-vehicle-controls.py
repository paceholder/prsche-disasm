"""Record V4 fixtures: the original controls (input word, steering rate,
gear bookkeeping, automatic R/N/1, auto-reverse, race-start gating).

Executes nfs5.exe in Unicorn (scripts/research/vehicle_harness.py, x87 control
word 0x27f) on synthetic cars:

- Car_ReadInputs 0x4991e0, cdecl (car): the input word g_InputWord [0x5e9664]
  (byte0 bit0 handbrake, bit1 horn, bits 3..7 event; byte1 steer (signed);
  byte2 gas/2; byte3 brake/2) -> d7c/d7d/d90/d85/d86, d8a (byte0), d8b
  (event), d8c (events 7..9), the d87 toggles with +0x1230/+0x1232 (events
  10..13, 15), and the gear request d7e from Input_GearRequest 0x413890
  (event 2 SHIFTUP -> gear+1 below sim+0x50-1, event 3 SHIFTDOWN -> gear-1
  above R), not stored when it is a downshift with db4 > redline
  (0x499446..0x499475). The mirror flag [0x65742c] = 1 negates the steer.
  Paths: player (da8 = 0, dac = 0), remote/replay (dac != 0: car+0x38 steer,
  +0x3a brake, +0x3b handbrake/horn bits; d7c is not written) and AI
  (da8 != 0: copies d9c/da0/d9d, d86 = 0). The AI planners 0x40b650 and
  0x40bfe0 and the remote lock pair 0x5322b0/0x5322c0 are stubbed as
  no-ops, so the AI rows cover only the copy. [0x5e8e28] = 0 skips
  0x516950 and [0x657408] = 0 skips 0x4a0680. With +0x52c bit 2 the
  callback +0xe10 runs first; it points to a no-op stub.
- Car_ApplyControls 0x49dd20, cdecl (car): race-start counter db0, kill
  conditions (db0 == 0, or +0x52c bit 2 with +0xb50 >= 2), d7f/d80 from
  d7c/d7d, shift timer d84 countdown, the automatic R/N/1 logic and
  auto-reverse ([0x658098 + player(+0x520)*0x580] == 1), manual requests,
  the steering rate (d94 toward d90: 12 per call, 18 with g_HandlingModeFlag
  [0x6573f8] or res+0x518, only if res+0x14; AI: (d90+d94)/2), race-start
  throttle/brake forcing, the +0x540/+0x544 and d2c == 2 overrides, the AI
  handbrake turn ([0x5e67b0], d94 = +-124 by sign of +0xeb0 * +0xed0,
  d85 = 1), and g_Throttle [0x628fb0], g_Brake [0x628fac], g_SteerAbs
  [0x628fb4] (divisor 252, 240 with [0x657404] > 0; swapped in R with d98).

Field meanings: docs/re/vehicle.md sections 2 and 3 (workspace root).

Cars: carSpecs from the V1 derivation (0x49c82f..0x49cb50, default tuning)
for 356Acoupe16 (4 gears), 993coupe436 (6 gears) and boxster25 (5 gears);
vehicle-controls-cars.tsv holds the 0x1c4 bytes in hex.

Rows: `call` = read (0x4991e0 only), apply (0x49dd20 only) or step (0x4991e0
then 0x49dd20, the order of the 32 Hz queue: prio 0x15 inputs, then
Car_PlanarDynamics calls 0x49dd20). Every row holds the whole state before
the call(s) and out_* after. Sequences (vehicle-controls-seq.tsv) are steps
on one car; between steps the script advances g_SchedStep by 2 (one 32 Hz
step = two 64 Hz scheduler steps), writes the input word, and moves the car
with a scripted speed trace from the outputs: v += (8 g_Throttle (forward
gear) | -5 g_Throttle (R) - 9 g_Brake sign(v), not crossing 0)/32, d60 = v,
+0x35c = |v|; db4 = |v| * ratio[d82] in gear, else towards g_Throttle *
redline by 400 per step.

Every write outside the stack is logged (UC_HOOK_MEM_WRITE) and checked
against the state columns, so the outputs are all the fields written.

    python scripts/research/replay-vehicle-controls.py --game-dir local/game \
        --out-dir local/fixtures
"""
import argparse
import math
from pathlib import Path
import random
import struct
import sys

from unicorn import UC_HOOK_MEM_WRITE

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vehicle_harness as vh  # noqa: E402

READ_INPUTS, APPLY_CONTROLS = 0x4991e0, 0x49dd20
STUBS_NOP = (0x40b650, 0x40bfe0, 0x5322b0, 0x5322c0)
CALLBACK_STUB = vh.EXIT + 0x100
G_INPUT_WORD, G_MIRROR, G_NET_PLAYERS = 0x5e9664, 0x65742c, 0x5e8e28
G_THROTTLE, G_BRAKE, G_STEER_ABS = 0x628fb0, 0x628fac, 0x628fb4
AUTO_BASE, PLAYER_STRIDE = 0x658098, 0x580

CARS = [('356A', '356Acoupe16.sim', '356A'), ('993C4', '993coupe436.sim', '993'),
        ('Boxster', 'boxster25.sim', 'Boxster')]

SHIFT_UP, SHIFT_DOWN = 2 << 3, 3 << 3

# (column, car offset, kind): f f32, i i32, b u8, s i8, h u16
CAR_FIELDS = [
    ('remote_steer_38', 0x38, 's'), ('remote_brake_3a', 0x3a, 'b'), ('remote_bits_3b', 0x3b, 'b'),
    ('speed_35c', 0x35c, 'f'), ('player_520', 0x520, 'i'), ('flags_52c', 0x52c, 'b'),
    ('field_540', 0x540, 'i'), ('field_544', 0x544, 'i'), ('field_b50', 0xb50, 'i'),
    ('field_d2c', 0xd2c, 'i'), ('vel_fwd_d60', 0xd60, 'f'),
    ('gas_d7c', 0xd7c, 'b'), ('brake_d7d', 0xd7d, 'b'), ('gear_req_d7e', 0xd7e, 's'),
    ('gas_eff_d7f', 0xd7f, 'b'), ('brake_eff_d80', 0xd80, 'b'), ('prev_gear_d81', 0xd81, 's'),
    ('gear_d82', 0xd82, 's'), ('downshift_d83', 0xd83, 'b'), ('shift_timer_d84', 0xd84, 's'),
    ('handbrake_d85', 0xd85, 'b'), ('horn_d86', 0xd86, 'b'), ('toggles_d87', 0xd87, 'b'),
    ('word0_d8a', 0xd8a, 'b'), ('event_d8b', 0xd8b, 'b'), ('event_d8c', 0xd8c, 'b'),
    ('steer_in_d90', 0xd90, 'i'), ('steer_d94', 0xd94, 'i'), ('reverse_d98', 0xd98, 'i'),
    ('ai_gas_d9c', 0xd9c, 'b'), ('ai_brake_d9d', 0xd9d, 'b'), ('ai_steer_da0', 0xda0, 'i'),
    ('ai_da8', 0xda8, 'i'), ('remote_dac', 0xdac, 'i'), ('start_db0', 0xdb0, 'i'),
    ('rpm_db4', 0xdb4, 'f'), ('ai_side_eb0', 0xeb0, 'i'), ('ai_side_ed0', 0xed0, 'f'),
    ('lights_1230', 0x1230, 'h'), ('lights_1232', 0x1232, 'h'),
]
# Globals (column, address, kind). `auto` is written at the player's slot.
GLOBAL_FIELDS = [
    ('input_word', G_INPUT_WORD, 'u'), ('mirror', G_MIRROR, 'i'),
    ('throttle', G_THROTTLE, 'f'), ('brake', G_BRAKE, 'f'), ('steer_abs', G_STEER_ABS, 'f'),
]
SETUP = ['res_14', 'res_518', 'sched_step', 'auto', 'handling', 'ai_turn', 'split']
STATE = [name for name, _, _ in CAR_FIELDS] + [name for name, _, _ in GLOBAL_FIELDS]


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def short(x):
    """Shortest decimal that parses back to the same f32 (Rust `parse::<f32>`)."""
    x = f32(x)
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


KIND_FMT = {'f': '<f', 'i': '<i', 'u': '<I', 'b': '<B', 's': '<b', 'h': '<H'}


class Recorder:
    def __init__(self, game_dir):
        module, data = vh.load_module('nfs5.exe', game_dir)
        stubs = {va: (lambda m: 0) for va in STUBS_NOP}
        stubs[CALLBACK_STUB] = lambda m: 0
        self.m = vh.Machine(module, data, stubs=stubs, trace_calls=True)
        self.writes = set()
        stack = (vh.STACK_TOP - 0x10000, vh.STACK_TOP)

        def on_write(_uc, _access, address, size, _value, _user):
            if not stack[0] <= address < stack[1]:
                for a in range(address, address + size):
                    self.writes.add(a)

        self.m.uc.hook_add(UC_HOOK_MEM_WRITE, on_write)
        self.specs = {}
        for name, sim, model in CARS:
            raw = (game_dir / 'GameData/Simulation/CarData' / sim).read_bytes()
            self.specs[name] = vh.derive_specs(self.m, raw, model)
        self.fields = {name: dict(vh.unpack_fields(self.specs[name], vh.CARSPECS_FIELDS))
                       for name, _, _ in CARS}
        self.allowed = set()
        for _, off, kind in CAR_FIELDS:
            size = struct.calcsize(KIND_FMT[kind])
            self.allowed |= set(range(vh.CAR + off, vh.CAR + off + size))
        for addr in (G_THROTTLE, G_BRAKE, G_STEER_ABS):
            self.allowed |= set(range(addr, addr + 4))

    def spec(self, car, name):
        return self.fields[car][name]

    def setup(self, p):
        m = self.m
        m.reset_car()
        m.set_globals(g_SchedStep=p['sched_step'], g_HandlingModeFlag=p['handling'],
                      g_AiHandbrakeTurn=p['ai_turn'], g_SplitScreen=p['split'], g_AutoBox=0)
        # The other player's slot gets the opposite value, so a wrong index shows.
        m.put_i32(AUTO_BASE, 1 - p['auto'])
        m.put_i32(AUTO_BASE + PLAYER_STRIDE, 1 - p['auto'])
        m.put_i32(AUTO_BASE + p['player_520'] * PLAYER_STRIDE, p['auto'])
        m.put_i32(G_NET_PLAYERS, 0)
        m.put_bytes(vh.SPECS, self.specs[p['car']])
        m.put_i32(vh.RES + 0x14, p['res_14'])
        m.put_i32(vh.RES + 0x518, p['res_518'])
        m.put_u32(vh.CAR + 0xe10, CALLBACK_STUB)
        for name, off, kind in CAR_FIELDS:
            m.uc.mem_write(vh.CAR + off, struct.pack(KIND_FMT[kind], p[name]))
        for name, addr, kind in GLOBAL_FIELDS:
            m.uc.mem_write(addr, struct.pack(KIND_FMT[kind], p[name]))

    def read_state(self):
        m = self.m
        out = {}
        for name, off, kind in CAR_FIELDS:
            fmt_ = KIND_FMT[kind]
            out[name] = struct.unpack(fmt_, m.get_bytes(vh.CAR + off, struct.calcsize(fmt_)))[0]
        for name, addr, kind in GLOBAL_FIELDS:
            out[name] = struct.unpack(KIND_FMT[kind], m.get_bytes(addr, 4))[0]
        return out

    def run(self, call, p):
        self.setup(p)
        self.writes = set()
        if call in ('read', 'step'):
            self.m.call(READ_INPUTS, vh.CAR)
        if call in ('apply', 'step'):
            self.m.call(APPLY_CONTROLS, vh.CAR)
        if not self.writes <= self.allowed:
            raise RuntimeError(f'unexpected writes {sorted(hex(a) for a in self.writes - self.allowed)}')
        return self.read_state()


def columns(call, p, out):
    row = [('case', p['case']), ('step', p.get('step', 0)), ('call', call), ('car', p['car'])]
    row += [(k, p[k]) for k in SETUP]
    row += [(k, f32(p[k]) if isinstance(p[k], float) else p[k]) for k in STATE]
    row += [(f'out_{k}', out[k]) for k in STATE]
    return row


def base(car, **kw):
    p = dict(case='', car=car, res_14=1, res_518=0, sched_step=0x1c4, auto=1, handling=0,
             ai_turn=0, split=0, input_word=0, mirror=0, throttle=0.0, brake=0.0, steer_abs=0.0)
    for name, _, kind in CAR_FIELDS:
        p[name] = 0.0 if kind == 'f' else 0
    p.update(gear_d82=1, gear_req_d7e=1, prev_gear_d81=1, start_db0=0x100)
    p.update(kw)
    return p


def word(gas=0, brake=0, steer=0, handbrake=0, horn=0, event=0):
    """Input word as Input_PollControllers packs it (gas/brake 0..254 halved,
    steer -127..127 halved, event code in bits 3..7)."""
    b0 = (handbrake & 1) | ((horn & 1) << 1) | (event & 0xf8)
    return b0 | ((steer // 2) & 0xff) << 8 | ((gas // 2) & 0xff) << 16 | ((brake // 2) & 0xff) << 24


# ---------------------------------------------------------------------------
# Single calls.
def read_cases(rec):
    rng = random.Random(0x4991e0)
    cases = []
    for _ in range(1500):
        car = rng.choice(CARS)[0]
        top = rec.spec(car, 'gear_count_50') - 1
        red = rec.spec(car, 'redline_94')
        gear = rng.randrange(0, top + 1)
        path = rng.choice(('player',) * 6 + ('remote', 'ai'))
        event = rng.choice((0, SHIFT_UP, SHIFT_DOWN, SHIFT_UP, SHIFT_DOWN, rng.randrange(32) << 3))
        b0 = rng.randrange(8) | event
        w = b0 | rng.randrange(1 << 24) << 8
        cases.append(base(
            car, case=f'read_{path}', input_word=w, mirror=rng.choice((0, 0, 1, 2)),
            ai_da8=int(path == 'ai') * rng.choice((1, 2)), remote_dac=int(path == 'remote'),
            flags_52c=rng.choice((0, 0, 4, 0xfb)), gear_d82=gear,
            gear_req_d7e=rng.choice((gear, rng.randrange(top + 1))),
            rpm_db4=rng.choice((f32(red), f32(red) + 0.5, red - 1, red * 1.1, rng.uniform(0, 1.3 * red))),
            toggles_d87=rng.randrange(256), lights_1230=rng.randrange(1 << 16),
            lights_1232=rng.randrange(1 << 16), event_d8c=rng.randrange(32),
            remote_steer_38=rng.randrange(-128, 128), remote_brake_3a=rng.randrange(256),
            remote_bits_3b=rng.randrange(256), ai_gas_d9c=rng.randrange(256),
            ai_brake_d9d=rng.randrange(256), ai_steer_da0=rng.randrange(-200, 200),
            gas_d7c=rng.randrange(256), brake_d7d=rng.randrange(256), steer_in_d90=rng.randrange(-300, 300),
            handbrake_d85=rng.randrange(2), horn_d86=rng.randrange(2), word0_d8a=rng.randrange(256),
            event_d8b=rng.randrange(32)))
    return cases


def apply_cases(rec):
    rng = random.Random(0x49dd20)
    cases = []
    for _ in range(4000):
        car = rng.choice(CARS)[0]
        top = rec.spec(car, 'gear_count_50') - 1
        gear = rng.choice((0, 1, 2, 2, rng.randrange(top + 1)))
        req = rng.choice((gear, gear, 0, 1, 2, rng.randrange(top + 1)))
        red = rec.spec(car, 'redline_94')
        v = rng.choice((0.0, 0.1, -0.15, 0.19999999, 0.2, -0.2, 0.25, rng.uniform(-10, 10),
                        rng.uniform(0, 1.0) * red * rec.spec(car, 'inv_ratio_170_3')))
        player = rng.choice((0, 0, 1))
        cases.append(base(
            car, case='apply', player_520=player, auto=rng.choice((1, 1, 0, 2)),
            sched_step=rng.choice((0x1c4, 0x1c4, 0x1c0, 0x1c1, 0x1c3, 0x100, 0x400)),
            handling=rng.choice((0, 0, 1)), ai_turn=rng.choice((0, 0, 0, 1)), split=rng.choice((0, 0, 1)),
            res_14=rng.choice((1, 1, 0)), res_518=rng.choice((0, 0, 1)),
            start_db0=rng.choice((0, 1, 0x3e, 0x3f, 0x40, 0x100, 0x100)),
            flags_52c=rng.choice((0, 0, 0, 4)), field_b50=rng.choice((0, 0, 1, 2)),
            field_540=rng.choice((0,) * 8 + (1,)), field_544=rng.choice((0,) * 8 + (1,)),
            field_d2c=rng.choice((0, 0, 0, 2, 1)), vel_fwd_d60=v,
            speed_35c=abs(v) * rng.choice((1.0, 1.0, 1.0, 2.0)),
            gas_d7c=rng.choice((0, 0, 0x80, 0x81, 254, rng.randrange(256))),
            brake_d7d=rng.choice((0, 0x80, 0x81, 254, rng.randrange(256))),
            gear_req_d7e=req, gas_eff_d7f=rng.randrange(256), brake_eff_d80=rng.randrange(256),
            prev_gear_d81=rng.randrange(top + 1), gear_d82=gear, downshift_d83=rng.randrange(2),
            shift_timer_d84=rng.choice((0, 0, 1, 5, -3)), handbrake_d85=rng.randrange(2),
            steer_in_d90=rng.choice((0, 254, -254, 7, -11, 12, 18, rng.randrange(-260, 260))),
            steer_d94=rng.choice((0, rng.randrange(-260, 260))),
            reverse_d98=rng.choice((0, 0, 1, 2)), ai_da8=rng.choice((0, 0, 0, 1)),
            ai_side_eb0=rng.choice((0, 1, -1, 3)), ai_side_ed0=rng.choice((0.0, 0.5, -2.0)),
            rpm_db4=rng.uniform(0, red)))
    return cases


# ---------------------------------------------------------------------------
# Sequences: Car_ReadInputs then Car_ApplyControls per 32 Hz step.
class Sequence:
    def __init__(self, rec, name, car, rows, **kw):
        self.rec, self.name, self.car, self.rows = rec, name, car, rows
        self.p = base(car, case=name, **kw)
        self.n = 0

    def step(self, w, rpm=None):
        rec, p = self.rec, self.p
        p['input_word'] = w
        p['step'] = self.n
        out = rec.run('step', p)
        self.rows.append(columns('step', p, out))
        for k in STATE:
            p[k] = out[k]
        # Scripted motion from g_Throttle / g_Brake.
        v, gear = p['vel_fwd_d60'], p['gear_d82']
        thr, brk = out['throttle'], out['brake']
        a = 8.0 * thr if gear >= 2 else (-5.0 * thr if gear == 0 else 0.0)
        v2 = f32(v + a / 32.0)
        dec = 9.0 * brk / 32.0
        v2 = f32(max(v2 - dec, 0.0) if v2 > 0 else min(v2 + dec, 0.0))
        p['vel_fwd_d60'], p['speed_35c'] = v2, abs(v2)
        red = rec.spec(self.car, 'redline_94')
        if rpm is not None:
            p['rpm_db4'] = f32(rpm)
        elif gear >= 2 or gear == 0:
            p['rpm_db4'] = f32(abs(v2) * rec.spec(self.car, f'gear_ratio_60_{gear}'))
        else:
            target = thr * red
            cur = p['rpm_db4']
            p['rpm_db4'] = f32(min(cur + 400, target) if cur < target else max(cur - 400, target))
        p['sched_step'] += 2
        self.n += 1
        return out


def sequences(rec):
    rows = []
    # Race start, automatic, in N and in 1st: full gas through the countdown.
    for case, gear, step0 in (('race_start_auto_n', 1, 0x100), ('race_start_auto_1st', 2, 0x100),
                              ('race_start_auto_late', 1, 0x1a0)):
        s = Sequence(rec, case, 'Boxster', rows, gear_d82=gear, gear_req_d7e=gear, start_db0=1,
                     sched_step=step0)
        for _ in range(0x100 if step0 == 0x100 else 0x40):
            s.step(word(gas=254, steer=40))
    # Race start, manual box: stays in N until SHIFTUP.
    s = Sequence(rec, 'race_start_manual', 'Boxster', rows, auto=0, start_db0=1, sched_step=0x140)
    for i in range(0x60):
        s.step(word(gas=254, event=SHIFT_UP if i == 0x50 else 0))
    for _ in range(20):
        s.step(word(gas=254))
    # Never started (db0 = 0): everything forced.
    s = Sequence(rec, 'not_started', '356A', rows, start_db0=0, gear_d82=2, gear_req_d7e=2)
    for _ in range(10):
        s.step(word(gas=254, steer=127))
    # Auto-reverse and back, automatic box.
    for car in ('Boxster', '993C4'):
        s = Sequence(rec, 'auto_reverse', car, rows, gear_d82=3, gear_req_d7e=3, prev_gear_d81=2,
                     vel_fwd_d60=6.0, speed_35c=6.0, sched_step=0x200)
        for _ in range(60):
            s.step(word(brake=254))          # stop, reverse, back up
        for _ in range(8):
            s.step(word())
        for _ in range(40):
            s.step(word(gas=254))            # gas brakes, then 1st at standstill
        for _ in range(30):
            s.step(word(gas=200))
        for _ in range(30):
            s.step(word(brake=0x80))         # brake 0x80 does not reverse
    # Light brake at standstill (not > 0x80) and brake with gas held.
    s = Sequence(rec, 'no_reverse', '356A', rows, gear_d82=2, gear_req_d7e=2, sched_step=0x200)
    for w in [word(brake=0x80)] * 6 + [word(brake=254, gas=2)] * 6 + [word(brake=254)] * 6:
        s.step(w)
    # Automatic: shift requests (down into N/R taken, up ignored except N/R).
    s = Sequence(rec, 'auto_requests', '993C4', rows, gear_d82=2, gear_req_d7e=2, sched_step=0x200)
    for ev in (SHIFT_UP, 0, SHIFT_DOWN, 0, SHIFT_DOWN, 0, SHIFT_DOWN, 0, SHIFT_UP, 0, SHIFT_UP, 0,
               SHIFT_UP, 0, 0, 0, 0, 0, 0, 0, 0):
        s.step(word(event=ev))
    # N -> 1st while rolling: the gear picked from d60.
    for v in (5.0, 15.0, 30.0, 45.0, 60.0):
        s = Sequence(rec, 'auto_pick', '993C4', rows, gear_d82=1, gear_req_d7e=1, sched_step=0x200,
                     vel_fwd_d60=v, speed_35c=v)
        for ev in (SHIFT_UP, 0, 0):
            s.step(word(event=ev))
    # Manual box: requests below, at and over redline.
    for car in ('993C4', '356A'):
        red = rec.spec(car, 'redline_94')
        s = Sequence(rec, 'manual_shifts', car, rows, auto=0, gear_d82=2, gear_req_d7e=2,
                     vel_fwd_d60=10.0, speed_35c=10.0, sched_step=0x200)
        script = [(SHIFT_UP, 0.5), (0, 0.5)] * 7 + [(SHIFT_DOWN, 0.6), (0, 0.6)] * 2
        script += [(SHIFT_DOWN, 1.0), (SHIFT_DOWN, 1.0001), (SHIFT_DOWN, 1.05), (0, 1.05)]
        script += [(SHIFT_UP, 1.1), (0, 1.1), (SHIFT_DOWN, 0.99), (0, 0.9)]
        script += [(SHIFT_DOWN, 0.5), (0, 0.5)] * 8 + [(SHIFT_UP, 0.0)] * 3
        for ev, frac in script:
            rpm = f32(red) if frac == 1.0 else red * frac
            s.step(word(gas=128, event=ev), rpm=rpm)
    # Steering rate: res+0x14 off (copy), on (12), on with handling / res+0x518 (18); mirror.
    pattern = [254] * 14 + [-254] * 26 + [0] * 14 + [30] * 3 + [-6] * 3 + [254] * 6
    for case, kw in (('steer_copy', dict(res_14=0)), ('steer_rate12', {}),
                     ('steer_rate18_handling', dict(handling=1)),
                     ('steer_rate18_light', dict(res_518=1)), ('steer_mirror', dict(mirror=1))):
        s = Sequence(rec, case, 'Boxster', rows, gear_d82=2, gear_req_d7e=2, sched_step=0x200, **kw)
        for st in pattern:
            s.step(word(gas=100, steer=st, handbrake=int(st == 0), horn=int(st < 0)))
    # Overrides +0x540/+0x544 (with and without d98) and d2c == 2.
    for case, kw in (('override_540', dict(field_540=1)), ('override_544_reverse',
                                                           dict(field_544=1, reverse_d98=1, gear_d82=0,
                                                                gear_req_d7e=0)),
                     ('override_d2c', dict(field_d2c=2))):
        s = Sequence(rec, case, '356A', rows, sched_step=0x200, **dict(dict(gear_d82=2, gear_req_d7e=2), **kw))
        for _ in range(6):
            s.step(word(gas=254, brake=60))
    return rows


def ai_cases(rec):
    """AI branch: Car_ReadInputs copies the planner outputs, Car_ApplyControls
    averages the steer and, with [0x5e67b0], makes the handbrake turn."""
    rng = random.Random(0x5e67b0)
    rows = []
    for _ in range(300):
        car = rng.choice(CARS)[0]
        p = base(car, case='ai_step', ai_da8=1, ai_turn=rng.choice((0, 1)),
                 auto=rng.choice((0, 1)), gear_d82=rng.choice((1, 2, 3)),
                 gear_req_d7e=rng.choice((1, 2, 3, 4)), ai_gas_d9c=rng.randrange(256),
                 ai_brake_d9d=rng.randrange(256), ai_steer_da0=rng.randrange(-127, 128),
                 steer_d94=rng.randrange(-127, 128), ai_side_eb0=rng.choice((-1, 0, 1, 2)),
                 ai_side_ed0=rng.choice((-1.5, 0.0, 0.25)), vel_fwd_d60=rng.uniform(0, 40),
                 sched_step=rng.choice((0x1c4, 0x1c2, 0x100)))
        p['speed_35c'] = p['vel_fwd_d60']
        rows.append(columns('step', p, rec.run('step', p)))
    return rows


def record(game_dir):
    rec = Recorder(game_dir)
    cars = [[('car', name), ('sim', sim), ('model', model), ('specs_hex', rec.specs[name].hex())]
            for name, sim, model in CARS]
    single = [columns('read', p, rec.run('read', p)) for p in read_cases(rec)]
    single += [columns('apply', p, rec.run('apply', p)) for p in apply_cases(rec)]
    single += ai_cases(rec)
    seq = sequences(rec)
    return cars, single, seq, dict(rec.m.calls)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    first = record(args.game_dir)
    second = record(args.game_dir)
    if first[:3] != second[:3]:
        raise RuntimeError('two runs differ')
    cars, single, seq, calls = first
    note = 'generated by scripts/research/replay-vehicle-controls.py (x87 control word 0x27f).'
    sizes = [
        write_tsv(args.out_dir / 'vehicle-controls-cars.tsv', cars, [
            'V4 cars: carSpecs (0x1c4 bytes, hex) from nfs5.exe 0x49c82f..0x49cb50, default tuning;',
            note]),
        write_tsv(args.out_dir / 'vehicle-controls-step.tsv', single, [
            'V4 single calls: call = read (Car_ReadInputs 0x4991e0), apply (Car_ApplyControls',
            '0x49dd20) or step (both); state columns before, out_* after; ' + note]),
        write_tsv(args.out_dir / 'vehicle-controls-seq.tsv', seq, [
            'V4 sequences: 0x4991e0 then 0x49dd20 per 32 Hz step; between steps g_SchedStep += 2,',
            'the input word from the script, scripted motion v += (8 thr | -5 thr in R)/32 minus',
            '9 brake/32 towards 0, db4 = |v| ratio[d82] in gear (else scripted); ' + note]),
    ]
    print('calls:', ', '.join(f'{t:#x} x{n}' for t, n in sorted(calls.items())))
    print(f'{len(single)} single rows, {len(seq)} sequence rows, {sum(sizes)} bytes; '
          f'two runs identical -> {args.out_dir}')


if __name__ == '__main__':
    main()
