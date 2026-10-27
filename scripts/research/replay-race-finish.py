"""Record the original race progress, finish and race-end bookkeeping by executing nfs5.exe.

Runs unchanged in Unicorn (vehicle_harness.Machine, x87 control word 0x27f) on synthetic
cars and a synthetic node table:

- Car_TrackProgressTick 0x4169b0 (every even step for every car) with Race_ProgressTick
  0x416310 (lap/finish bookkeeping, race order table 0x608b38), Car_ProgressLapCount
  0x417440 and the node lap fraction 0x481b70 (thiscall on the node object [0x628bc0]);
- Race_EndCheck 0x417010 (odd steps, from Sched_Step 0x414ff0, followed by the places 0x415fc0): ends the race loop
  (0x606a8c = 1) 5 s after the last human finished (10 s while speech is busy) and then
  runs Race_FinalResults 0x416ac0(2) (places +0xcd4, times +0xcd8, best lap +0xcfc,
  estimated times of the cars still racing).

Each car is set up by the original race-field reset 0x40f0b0 (from 0x410730), then the
start/finish fractions +0xb00/+0xb04/+0xb08 of the case are written.
Stubbed (recorded as calls): Car_ResetToTrack 0x40f540 (behind the start of a point-to-point
race), the lap speech 0x4277d0, the network 0x48da60, the part wear 0x493cf0.
Single player, not network ([0x628c70] = 0), race type 0 ([0x6573ec]), game type 0
([0x6573e8]), no DERBY ([0x657400] = 0). Synthetic inputs only, so the fixture lives in the
engine repo: crates/nfs-assets/tests/fixtures/race-finish-steps.tsv.

    PYTHON scripts/research/replay-race-finish.py --game-dir local/game \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/race-finish-steps.tsv

Rows: one per (case, step, car) after the step's progress ticks and end check: the car's
race fields, the order table entry of that rank, the globals, and the stub calls of the step.
Meanings: docs/re/race-finish.md in the workspace root.
"""
import argparse
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from vehicle_harness import Machine, load_module, write_tsv  # noqa: E402

CARS = 0x3000000          # 8 car structs, 0x1400 apart
CAR_STRIDE = 0x1400
NODES = 0x3010000         # node object and section table
PARTS = 0x3012000

PROGRESS_TICK, END_CHECK, ORDER_CLEAR, PLACES = 0x4169b0, 0x417010, 0x416140, 0x415fc0
CAR_RACE_RESET = 0x40f0b0
STUBS = {0x40f540: 'reset', 0x4277d0: 'lap_speech', 0x48da60: 'net', 0x493cf0: 'wear'}

G = {
    'step': 0x606ac4, 'game_type': 0x6573e8, 'race_type': 0x6573ec, 'laps': 0x6573f0,
    'derby': 0x657400, 'net_mode': 0x657408, 'player': 0x65741c, 'reverse': 0x657430,
    'linear': 0x657494, 'network': 0x628c70, 'nodes': 0x628bc0, 'cars': 0x5e8ae8,
    'humans': 0x5e8bfc, 'opponents': 0x5e8bf8, 'players': 0x658084, 'camera_mode': 0x606850,
    'leave': 0x606a8c, 'speech_w': 0x656ae4, 'speech_r': 0x657300, 'speech_a': 0x6573a8,
    'speech_b': 0x6573ac, 'mode_424': 0x657424, 'catchup': 0x657420,
}
ALL_CARS, HUMAN_CARS, OPPONENT_CARS = 0x5e8e2c, 0x5e8c00, 0x5e8bd4
ORDER = 0x608b38


def car(i):
    return CARS + i * CAR_STRIDE


def build(m, n_nodes, sections):
    """Node object: +4 nodes per lap, +0x18 -> {begin, end} of 0x28-byte section records
    {first, ?, ?, ?, last, ?, ?, ?, start node object, end node object}; node object +0x28 =
    lap fraction (0x481b70)."""
    m.put_u32(G['nodes'], NODES)
    m.put_i32(NODES + 4, n_nodes)
    m.put_u32(NODES + 0x18, NODES + 0x20)
    recs = NODES + 0x100
    m.put_u32(NODES + 0x20, recs)
    m.put_u32(NODES + 0x24, recs + 0x28 * len(sections))
    frac = NODES + 0x800
    for k, (first, last, f0, f1) in enumerate(sections):
        r = recs + 0x28 * k
        m.put_i32(r, first)
        m.put_i32(r + 0x10, last)
        m.put_u32(r + 0x20, frac + 0x60 * k)
        m.put_u32(r + 0x24, frac + 0x60 * k + 0x30)
        m.put(frac + 0x60 * k + 0x28, f0)
        m.put(frac + 0x60 * k + 0x30 + 0x28, f1)


def setup(m, case):
    m.uc.mem_write(CARS, bytes(0x10000))
    m.uc.mem_write(NODES, bytes(0x2000))
    n = case['cars']
    build(m, case['nodes'], case['sections'])
    for name, value in (('game_type', 0), ('race_type', 0), ('laps', case['laps']), ('derby', 0),
                        ('net_mode', 0), ('player', 0), ('reverse', 0),
                        ('linear', case['linear']), ('network', 0), ('cars', n), ('humans', 1),
                        ('opponents', n - 1), ('players', 1), ('camera_mode', 0), ('leave', 0),
                        ('speech_w', 0), ('speech_r', 0), ('speech_a', -1), ('speech_b', -1),
                        ('mode_424', 0), ('catchup', 0), ('step', case['start_step'])):
        m.put_u32(G[name], value)
    for k in range(8):
        m.put_u32(ALL_CARS + 4 * k, car(k) if k < n else 0)
        m.put_u32(OPPONENT_CARS + 4 * k, car(k + 1) if k + 1 < n else 0)
    m.put_u32(HUMAN_CARS, car(0))
    m.uc.mem_write(ORDER, bytes(16 * 8))
    for k in range(n):
        c = car(k)
        m.call(CAR_RACE_RESET, c)          # 0x40f0b0: race fields, +0xb24 = 0x1c0
        m.put_i32(c + 0x520, k)
        m.put_u32(c + 0x52c, 5 if k == 0 else 1)     # human: bits 0 and 2
        m.uc.mem_write(c + 0x52c + 0, bytes([5 if k == 0 else 1]))
        m.put_u32(c + 0x7f, 1)                         # +0x7f active (byte)
        b00, b04, b08 = case['b00_b04_b08']
        m.put(c + 0xb00, b00)
        m.put(c + 0xb04, b04)
        m.put(c + 0xb08, b08)
        m.put_i32(c + 0x530, 0)
        m.put_i32(c + 0x534, 0)
        for p in range(20):
            m.put_u32(c + 0x768 + 4 * p, PARTS)
        # Initial order table entry (0x410730 race setup leaves cars in grid order).
        m.put_i32(ORDER + 16 * k, k)
        m.put_i32(ORDER + 16 * k + 8, 0)
    m.uc.mem_write(PARTS, bytes(0x100))


def run_case(m, case, rows, calls):
    setup(m, case)
    n = case['cars']
    left = None
    for s in range(case['steps']):
        if left is not None and s > left + 3:
            break
        step = case['start_step'] + s
        m.put_u32(G['step'], step)
        busy = case.get('speech_busy', (0, 0))
        m.put_i32(G['speech_a'], 5 if busy[0] <= step < busy[1] else -1)
        calls.clear()
        car_calls = [[] for _ in range(n)]
        # Sched_Step 0x414ff0: even steps clear the order table (0x416140) before the car
        # queue (Car_TrackProgressTick at prio 0x19); odd steps run the end check 0x417010
        # and the places 0x415fc0 after the queue.
        if step % 2 == 0:
            m.call(ORDER_CLEAR)
            for k in range(n):
                c = car(k)
                node = case['path'](k, step)
                m.uc.mem_write(c + 8, struct.pack('<h', node))
                m.put(c + 0xd60, case['speed'](k, step))
                m.call(PROGRESS_TICK, c)
                car_calls[k] = calls[:]
                calls.clear()
        else:
            m.call(END_CHECK)
            m.call(PLACES)
        end_log = ' '.join(calls) or '-'
        if left is None and m.get_i32(G['leave'])[0]:
            left = s
        for k in range(n):
            c = car(k)
            ri = m.get_i32(c + 0x530, 2)
            ints = {o: m.get_i32(c + o)[0] for o in (0xb10, 0xb14, 0xb18, 0xb20, 0xb24, 0xb48, 0xb50,
                                                      0xcd0, 0xcd4, 0xcd8, 0xcfc)}
            laps = m.get_i32(c + 0xb28, 8)
            od = struct.unpack('<ifii', m.get_bytes(ORDER + 16 * k, 16))
            rows.append([
                ('case', case['name']), ('step', step), ('car', k),
                ('node', struct.unpack('<h', m.get_bytes(c + 8, 2))[0]),
                ('speed_d60', m.get(c + 0xd60)[0]),
                ('af4', m.get(c + 0xaf4)[0]), ('af8', m.get(c + 0xaf8)[0]),
                ('laps_530', ri[0]), ('back_534', ri[1]),
                ('b10', ints[0xb10]), ('b14', ints[0xb14]), ('b18', ints[0xb18]),
                ('b20', ints[0xb20]), ('b24', ints[0xb24]),
                ('b28', ','.join(str(v) for v in laps)),
                ('top_ad4', ','.join(f'{v:.9g}' for v in m.get(c + 0xad4, 8))),
                ('state_b50', ints[0xb50]),
                ('cd0', ints[0xcd0]), ('place_cd4', ints[0xcd4]), ('time_cd8', ints[0xcd8]),
                ('best_cfc', ints[0xcfc]),
                ('order_id', od[0]), ('order_af8', od[1]), ('order_b14', od[2]), ('order_human', od[3]),
                ('camera_606850', m.get_i32(G['camera_mode'])[0]),
                ('leave_606a8c', m.get_i32(G['leave'])[0]),
                ('calls', ' '.join(car_calls[k]) or '-'), ('end_calls', end_log),
            ])


def linear_path(speeds, back=None):
    def path(k, step):
        t = (step - 0x1c0) // 2
        if t <= 0:
            return 10
        if back and k == back[0] and back[1] <= t < back[2]:
            return max(0, 10 - (t - back[1]))
        return min(10 + speeds[k] * t // 4, 199)
    return path


def circuit_path(speeds, back=None):
    def path(k, step):
        t = (step - 0x1c0) // 2
        if t <= 0:
            return 0
        pos = speeds[k] * t // 4
        if back and k == back[0] and back[1] <= t < back[2]:
            pos = speeds[k] * back[1] // 4 - (t - back[1]) * 3
        return pos % 100
    return path


CASES = [
    # Point-to-point: nodes 0..199, start node 10 (fraction 0.05), finish trigger node 190
    # (b04 = 0.95). Car 0 is the human; car 2 runs backwards behind the start once.
    dict(name='linear', cars=4, laps=1, linear=1, nodes=200, sections=[(0, 199, 0.0, 1.0)],
         b00_b04_b08=(0.05, 0.95, 0.045), start_step=0x1b8, steps=1500,
         path=linear_path([9, 8, 7, 3], back=(2, 2, 12)), speed=lambda k, s: 20.0 + k + (s % 7)),
    # Circuit, 3 laps, 100 nodes; the human crosses the line backwards once.
    dict(name='circuit', cars=3, laps=3, linear=0, nodes=100, sections=[(0, 99, 0.0, 1.0)],
         b00_b04_b08=(0.0, 1.0, -1.0), start_step=0x1b8, steps=1800,
         path=circuit_path([7, 6, 2], back=(0, 20, 30)), speed=lambda k, s: 30.0 - k + (s % 5)),
    # Point-to-point while speech is busy: the end waits 0x280 steps instead of 0x140.
    dict(name='linear_speech', cars=2, laps=1, linear=1, nodes=200, sections=[(0, 199, 0.0, 1.0)],
         b00_b04_b08=(0.05, 0.95, 0.045), start_step=0x1b8, steps=1400,
         path=linear_path([9, 4]), speed=lambda k, s: 25.0, speech_busy=(0, 0x10000)),
]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    module, data = load_module('nfs5.exe', a.game_dir)
    calls = []

    def stub(name):
        def h(_m):
            calls.append(name)
            return 0
        return h

    m = Machine(module, data, stubs={va: stub(n) for va, n in STUBS.items()})
    m.uc.mem_map(CARS, 0x20000)
    rows = []
    for case in CASES:
        run_case(m, case, rows, calls)
    write_tsv(a.out, rows, comments=[
        'replay-race-finish.py: Car_TrackProgressTick 0x4169b0 (0x416310, 0x417440, 0x481b70) '
        'and Race_EndCheck 0x417010 (0x416ac0) run unchanged; synthetic cars and nodes.'])
    print(f'{len(rows)} rows -> {a.out}')


if __name__ == '__main__':
    main()
