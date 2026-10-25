"""Record the original keyboard-to-wheel chain: raw key state -> input word ->
Car_ReadInputs -> Car_ApplyControls -> planar step -> front wheel angle.

Executes nfs5.exe in Unicorn (scripts/research/vehicle_harness.py, x87
control word 0x27f) on top of the V5 step recorder (replay-vehicle-step.py):
the same synthetic car on a flat plane, but the input word is not written by
the script. The original code produces it from the key state:

- 128 Hz timer (TIMER_init 0x565030 with 0x80 from 0x4a5444; callback list
  0x6b7c20): each tick runs the original callback Race_TimerTick 0x4a45d0
  (registered by 0x4a4700, also run). Single player (0x6573e8 != 3): it counts
  ticks 0x6559e8 and 0x6559e0, every 2nd tick the 64 Hz clock 0x6559ec, and on
  every 2nd 64 Hz count (32 Hz) calls Input_PollControllers 0x412e70 and
  Input_QueueLocal 0x413800 (the poll writes the player word 0x5e99dc; the
  queue push 0x415e80 stores it in ring slot 0x606af0[n & 0x7f] and counts
  0x5cc818).
- Key state: the window procedure stores 1/0 in the scancode table 0x5de028
  on WM_KEYDOWN/WM_KEYUP [VA 0x53b4a8, 0x53b6ee] (not run: the recorder writes
  the bytes). The bindings are written as the config loader stores
  `key,SC_x` (type 3 = "Key" in the device table 0x5cc5c8 {name, getter,
  release}; arg = scancode, bits 16..18 = shift/ctrl/alt): player 0 slots
  0x5e9668.. COMMAND1, COMMAND2, LEFT, RIGHT, GAS, BRAKE, HANDBRAKE, HORN
  (default keys fe.txt: SC_LEFT 0x4b, SC_RIGHT 0x4d, SC_UP 0x48, SC_DOWN
  0x50, SC_SPACE 0x39, SC_H 0x23); everything else unbound. No joystick
  (0x5e8e5c = 0), one player (0x658084 = 1), not split screen.
- Main loop (0x4152e0, an endless loop, not run; its step scheduling is
  reproduced): after each tick the last step allowed is 2 * [0x5cc818],
  plus 1 when the step count fell behind the 64 Hz clock; every step s up to
  it runs, on even s Input_PopStep 0x415ef0(s >> 1) (original: copies ring
  slot s/2 into 0x608b14[player], frees the slot) and then the V5 step (even:
  Car_ReadInputs 0x4991e0 -> the real +0xe10 callback Car_Callback_E10
  0x414810 -> Replay_RecordInput 0x4143f0 -> 0x413870 copies 0x608b14 into
  g_InputWord 0x5e9664; then Car_DynamicsTick 0x49fd30 and the flat ground;
  every s: 0x49b7a0, 0x49b8e0). The ring is reset by Input_QueueReset
  0x415e00 (slot 0 = word 0, push index 1), as at race start.

Car: as V5 (replay-vehicle-step.py Recorder.setup), plus +0x52c bit 2 (the
human player flag that enables the +0xe10 input callback; 0x410de9 installs
0x414810), +0x524 = 0 (player index), 0x5e8c00[0] = car, replay mode
0x5e99f4 = 0 (record). Race started (db0 0x100, s from 0x200).

Files (vehicle-keyboard-*.tsv, private: reads .sim data):
- runs: run name, car, v0, steps;
- ticks: per 128 Hz tick the key state and, when the tick polled, the word
  0x5e99dc it produced and the queue count;
- dyn: per even 64 Hz step the popped word 0x608b14, g_InputWord, the
  controls (d7c, d7d, d90, d94, d85), the V5 dyn values (steer_ac8, wheels
  steer_b4 ...);
- body: per 64 Hz step the V5 body values.

Two runs of the recorder are compared (determinism).

    python scripts/research/replay-vehicle-keyboard.py --game-dir local/game \
        --out-dir local/fixtures
"""
import argparse
import importlib.util
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vehicle_harness as vh  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    'replay_vehicle_step', Path(__file__).resolve().parent / 'replay-vehicle-step.py')
step = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(step)

TIMER_TICK, TIMER_START = 0x4a45d0, 0x4a4700
QUEUE_RESET, POP_STEP = 0x415e00, 0x415ef0
CALLBACK_E10 = 0x414810
G_KEYS = 0x5de028            # 256 bytes, 1 = held (scancode index)
G_BINDINGS = 0x5e9668        # player 0 controller slots (dwords)
BINDING_DWORDS = 0x50 * 2 + 0x30
G_POLL_WORD = 0x5e99dc       # Input_PollControllers output, player 0
G_POPPED = 0x608b14          # Input_PopStep output, player 0
G_QUEUED = 0x5cc818          # words pushed (single player)
G_CLOCK64 = 0x6559ec
G_CAR_PTRS = 0x5e8c00

KEY = 3                      # device type "Key" (0x5cc5c8 + 3*12)
SC = {'left': 0x4b, 'right': 0x4d, 'gas': 0x48, 'brake': 0x50, 'handbrake': 0x39, 'horn': 0x23}
# Modifier scancodes read by the Key getter 0x411c20 (shift 0x2a/0x36, ctrl 0x1d, alt 0x38):
# a held modifier that the binding does not ask for makes the key read as released.
MODS = {'shift': 0x2a}
SLOTS = ['command1', 'command2', 'left', 'right', 'gas', 'brake', 'handbrake', 'horn']

# Globals set per run (address, value) besides the V5 ones.
INPUT_GLOBALS = [
    (0x5e8e5c, 0),   # joystick count (Input_ReadJoysticks 0x411910 loop)
    (0x658084, 1),   # players
    (0x657404, 0),   # split screen
    (0x5cc81c, 0),   # paused queue
    (0x606a8c, 0),   # race over
    (0x65741c, 0),   # local player index
    (0x606aa4, 0),   # options menu open (the main loop polls itself then)
    (0x655a08, 0),   # timer callback disabled
    (0x655a04, 0),   # timer callback registered
    (0x6573e8, 0),   # game mode (3 = network)
    (0x5e99f4, 0),   # replay mode: 0 record
    (0x5e99f8, 0),   # replay buffer write offset
    (0x5e9a00, 0),   # replay cursor player 0
    (0x5e8e28, 0),   # network players (Car_ReadInputs 0x516950 when > 0)
]

# Writes outside the car the chain may do (address, size).
INPUT_WRITES = [
    (0x5cc818, 8), (0x5e9380, 0x100), (0x5e954c, 0x80), (0x5e9548, 4), (0x5e95cc, 8),
    (0x5e95d4, 0x120), (0x5e99b8, 0x48), (0x5e9664, 4), (0x5e9a00, 8), (0x5e8e58, 4),
    (0x5ed848, 0x19000), (0x606880, 0x20), (0x606af0, 0x2030), (0x6559e0, 0x24),
    (0x655a00, 0x10), (0x6b7c20, 0x20),
]

TICK_HZ = 128


def keys_down(*names):
    return frozenset(names)


def scenarios():
    """(name, car, v0, 64 Hz steps, keys(tick) -> set of held keys)."""
    t = lambda s: int(round(s * TICK_HZ))  # noqa: E731

    def hold_release(key, on, off, extra=()):
        return lambda k: keys_down(key, *extra) if t(on) <= k < t(off) else keys_down(*extra)

    def flick(k):
        if t(0.1) <= k < t(0.35):
            return keys_down('right')
        if t(0.35) <= k < t(0.85):
            return keys_down('left')
        if t(0.85) <= k < t(1.1):
            return keys_down('right')
        return keys_down()

    def both(k):
        if t(0.1) <= k < t(0.6):
            return keys_down('right')
        if t(0.6) <= k < t(1.0):
            return keys_down('right', 'left')
        if t(1.0) <= k < t(1.4):
            return keys_down('left')
        return keys_down()

    def taps(k):
        # A tap between two polls (ticks 21..22; polls at ticks 4n), a one-poll tap,
        # then taps of 1..4 polls with gaps.
        if 21 <= k < 23:
            return keys_down('right')
        if 40 <= k < 44:
            return keys_down('right')
        for n, start in enumerate((64, 96, 136, 184)):
            if start <= k < start + 4 * (n + 1):
                return keys_down('left')
        return keys_down()

    def pedals(k):
        held = set()
        if k < t(1.0):
            held.add('gas')
        if t(0.5) <= k < t(1.5):
            held.add('right')
        if t(0.8) <= k < t(1.5):
            held.add('brake')   # with gas until 1.0 s: both pedals
        if t(1.2) <= k < t(1.4):
            held.add('handbrake')
        return frozenset(held)

    def shift(k):
        held = {'right'} if t(0.1) <= k < t(1.2) else set()
        if t(0.4) <= k < t(0.7):
            held.add('shift')
        return frozenset(held)

    return [
        ('hold_right_0', '356A', 0.0, 192, hold_release('right', 0.1, 1.5)),
        ('hold_right_30', '356A', 30.0, 192, hold_release('right', 0.1, 1.5)),
        ('hold_left_30_gas', 'Boxster', 30.0, 192, hold_release('left', 0.1, 1.5, ('gas',))),
        ('flick_20', '356A', 20.0, 128, flick),
        ('flick_0', 'Boxster', 0.0, 128, flick),
        ('both_keys_15', '993C4', 15.0, 128, both),
        ('taps_25', '356A', 25.0, 128, taps),
        ('pedals_turn', 'Boxster', 0.0, 128, pedals),
        ('shift_right_20', '356A', 20.0, 160, shift),
    ]


class KeyboardRecorder(step.Recorder):
    def __init__(self, game_dir):
        super().__init__(game_dir)
        # memcpy 0x5323e0 calls EA's copy loop in the PACODE section (0x5b0000..0x5b1af9).
        self.m.code = (self.m.code[0], 0x5b1af9)
        for addr, size in INPUT_WRITES:
            self.allowed |= set(range(addr, addr + size))

    def setup_input(self):
        m = self.m
        for addr, value in INPUT_GLOBALS:
            m.put_u32(addr, value)
        m.put_bytes(G_KEYS, bytes(256))
        m.put_bytes(G_BINDINGS, bytes(4 * BINDING_DWORDS))
        for i, name in enumerate(SLOTS):
            if name in SC:
                m.put_u32(G_BINDINGS + 4 * i, KEY | SC[name] << 8)
        m.put_u32(G_CAR_PTRS, vh.CAR)
        m.put_u32(vh.CAR + 0x524, 0)
        flags = m.get_bytes(vh.CAR + 0x52c, 1)[0]
        m.put_bytes(vh.CAR + 0x52c, [flags | 4])
        m.put_u32(vh.CAR + 0xe10, CALLBACK_E10)
        m.call(QUEUE_RESET)
        m.call(TIMER_START)

    def set_keys(self, held):
        for name, sc in list(SC.items()) + list(MODS.items()):
            self.m.put_bytes(G_KEYS + sc, [1 if name in held else 0])

    def run_keys(self, name, car, v0, steps, keys):
        m = self.m
        run = f'{car}/{name}'
        gear = self.setup(car, v0, 0.0, 1)
        rpm0, speed0 = self.f(0xdb4), self.f(0x35c)
        self.setup_input()
        self.written = set()
        ticks, body, dyn = [], [], []
        n = 0                       # next 64 Hz step (main loop 0x606aa0)
        last_step, last_clock = 0, 0
        tick = 0
        while n < steps:
            held = keys(tick)
            self.set_keys(held)
            queued = m.get_u32(G_QUEUED)
            m.call(TIMER_TICK)
            polled = m.get_u32(G_QUEUED) != queued
            row = [('run', run), ('tick', tick)]
            row += [(k, int(k in held)) for k in list(SC) + list(MODS)]
            row += [('polled', int(polled)),
                    ('poll_word', m.get_u32(G_POLL_WORD) if polled else 0),
                    ('queued', m.get_u32(G_QUEUED))]
            ticks.append(row)
            # Main loop step target (0x4152e0, 0x415495..0x4154b4).
            target = 2 * m.get_u32(G_QUEUED)
            clock = m.get_u32(G_CLOCK64)
            if target - last_step < clock - last_clock:
                target += 1
            last_step, last_clock = target, clock
            while n <= target and n < steps:
                s = step.SCHED0 + n
                m.put_u32(step.G_SCHED, s)
                if s % 2 == 0:
                    m.call(POP_STEP, n >> 1)
                    popped = m.get_u32(G_POPPED)
                    m.call(step.READ_INPUTS, vh.CAR)
                    w = m.get_u32(step.G_INPUT_WORD)
                    m.call(step.DYNAMICS_TICK, vh.CAR)
                    self.ground()
                    d = self.dyn_row(run, n, w)
                    d.insert(2, ('tick', tick))
                    d.insert(3, ('popped', popped))
                    d += [('gas_d7c', self.b(0xd7c)), ('brake_d7d', self.b(0xd7d)),
                          ('steer_in_d90', self.i(0xd90))]
                    dyn.append(d)
                m.call(step.INTEGRATE_POS, vh.CAR)
                m.call(step.INTEGRATE_ORI, vh.CAR)
                body.append(self.body_row(run, n))
                n += 1
            tick += 1
        bad = self.written - self.allowed
        if bad:
            raise RuntimeError(f'{run}: unexpected writes {sorted(hex(a) for a in bad)[:20]}')
        runs = [('run', run), ('car', car), ('v0', vh.f32(v0)), ('wy0', 0.0), ('gear0', gear),
                ('rpm0', rpm0), ('speed0', speed0), ('steps', steps),
                ('sched0', step.SCHED0), ('rand_state0', vh.GLOBALS['g_RandState'][1]), ('rand_mul', 1),
                ('flags_52c', self.b(0x52c)), ('ticks', tick)]
        return runs, ticks, body, dyn


def record(game_dir):
    rec = KeyboardRecorder(game_dir)
    runs, ticks, body, dyn = [], [], [], []
    for sc in scenarios():
        r, t, b, d = rec.run_keys(*sc)
        runs.append(r)
        ticks += t
        body += b
        dyn += d
    return (runs, ticks, body, dyn), dict(rec.m.calls)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    first, calls = record(args.game_dir)
    second, _ = record(args.game_dir)
    if first != second:
        raise RuntimeError('two runs differ')
    runs, ticks, body, dyn = first
    note = 'generated by scripts/research/replay-vehicle-keyboard.py (x87 control word 0x27f).'
    out = args.out_dir
    sizes = [
        step.write_tsv(out / 'vehicle-keyboard-runs.tsv', runs, ['Keyboard chain runs; ' + note]),
        step.write_tsv(out / 'vehicle-keyboard-ticks.tsv', ticks, [
            'Keyboard chain: key state per 128 Hz tick; poll_word = 0x5e99dc after 0x412e70 on polling ticks;',
            note]),
        step.write_tsv(out / 'vehicle-keyboard-dyn.tsv', dyn, [
            'Keyboard chain: per even 64 Hz step the popped word 0x608b14, g_InputWord 0x5e9664 after',
            '0x4991e0, and the state after 0x49fd30 + flat ground; ' + note]),
        step.write_tsv(out / 'vehicle-keyboard-body.tsv', body, [
            'Keyboard chain: state after each 64 Hz step; ' + note]),
    ]
    print('calls:', ', '.join(f'{t:#x} x{n}' for t, n in sorted(calls.items())))
    print(f'{len(runs)} runs, {len(ticks)} ticks, {len(dyn)} dyn rows, {len(body)} body rows, '
          f'{sum(sizes)} bytes; two runs identical -> {out}')


if __name__ == '__main__':
    main()
