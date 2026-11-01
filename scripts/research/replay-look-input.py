"""Record the original look-behind / cycle-camera input chain: raw key state ->
interface bit word -> held look flags and the camera-cycle edge.

Executes nfs5.exe in Unicorn on top of the keyboard recorder
(replay-vehicle-keyboard.py, same machine, timer and bindings), with the two
default camera keys of fe.txt bound as the config loader stores them
(`Fe_ApplyRecord` 0x4b4b80: slot 0x5e9130 + op*4, `device | value << 8`; the
op is the first word of the {op, target, name} record the table builder at
0x4b30e0.. writes, e.g. [0x5d35e0] = 0x201 for the name at [0x5d35e8]):

- I_LOOKBEHIND  (op 0x201) = key,SC_B (scancode 0x30)
- I_CYCLECAMERA (op 0x205) = key,SC_C (scancode 0x2e)

Each 128 Hz tick runs the original Race_TimerTick 0x4a45d0; on its 32 Hz
polling ticks Input_PollControllers 0x412e70 increments the poll counter
0x5e99b8 and stores the interface bit word in 0x5e954c[0x5e99b8 & 0x1f]
(bit i set while the binding of op 0x1ee + i reports > 0x40; the 32
interface slots start at 0x5e98e8 = op 0x1ee MAINEXIT).

Main loop (0x4152e0, not run, an endless loop): for every new poll index it
sets 0x606ac8 to that index and, outside the pause menu, reads the look keys
with 0x4138d0(bit, 0) (level: bit of the current word) and the camera cycle
with 0x4138d0(0x17, 1) (edge: set now, clear in the previous word)
[VA 0x4152e0: g_LookForward 0x5e99a8 = bit 0x16, g_LookRight 0x5e99ac = 0x15,
g_LookBehind 0x5e99b0 = 0x13, g_LookLeft 0x5e99b4 = 0x14; Camera_Cycle
0x465a60 on bit 0x17]. The recorder reproduces that loop and calls the
original 0x4138d0 for each bit.

Files (camera-look-input.tsv): per polling tick the held keys, the poll
index, the interface word and the 0x4138d0 results.

    python scripts/research/replay-look-input.py --game-dir local/game \
        --out-dir local/fixtures
"""
import argparse
import importlib.util
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import vehicle_harness as vh  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    'replay_vehicle_keyboard', Path(__file__).resolve().parent / 'replay-vehicle-keyboard.py')
kb = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(kb)

INPUT_BIT = 0x4138d0          # (bit, edge) -> 0/1 on 0x5e954c[0x606ac8 & 0x1f]
G_POLL_INDEX = 0x5e99b8       # incremented by every poll
G_POLL_WORDS = 0x5e954c       # 32 interface words
G_MAIN_INDEX = 0x606ac8       # main loop's poll index
BINDING_BASE = 0x5e9130       # slot of op n = BINDING_BASE + 4n
OPS = {'look_behind': 0x201, 'cycle_camera': 0x205}
SC = {'look_behind': 0x30, 'cycle_camera': 0x2e, 'shift': 0x2a}
# bit -> column (Race_MainLoop 0x4152e0)
LEVEL_BITS = {0x13: 'g_look_behind', 0x14: 'g_look_left', 0x15: 'g_look_right', 0x16: 'g_look_forward'}
EDGE_BITS = {0x17: 'cycle_edge'}


def scenarios():
    """(name, ticks, keys(tick) -> set of held keys)."""
    t = lambda s: int(round(s * kb.TICK_HZ))  # noqa: E731

    def spans(*items):
        def held(k):
            out = set()
            for key, on, off in items:
                if t(on) <= k < t(off):
                    out.add(key)
            return frozenset(out)
        return held

    return [
        # Hold B half a second: set every poll while held, clear after (no toggle).
        ('hold_b', 128, spans(('look_behind', 0.1, 0.6))),
        # Two taps, one of them shorter than a poll gap (ticks 4n are polls).
        ('taps_b', 96, lambda k: frozenset({'look_behind'}) if 9 <= k < 11 or 20 <= k < 32 else frozenset()),
        # Shift held over part of the hold: the Key getter reads B as released.
        ('shift_b', 128, spans(('look_behind', 0.1, 0.8), ('shift', 0.3, 0.5))),
        # C held: one cycle edge per press; B and C together.
        ('cycle_c', 128, spans(('cycle_camera', 0.1, 0.4), ('cycle_camera', 0.5, 0.55),
                              ('look_behind', 0.45, 0.7))),
    ]


class LookRecorder(kb.KeyboardRecorder):
    def setup_look(self):
        m = self.m
        self.setup('356A', 0.0, 0.0, 1)
        self.setup_input()
        for name, op in OPS.items():
            m.put_u32(BINDING_BASE + 4 * op, kb.KEY | SC[name] << 8)
        m.put_u32(G_MAIN_INDEX, m.get_u32(G_POLL_INDEX))

    def set_look_keys(self, held):
        for name, sc in SC.items():
            self.m.put_bytes(kb.G_KEYS + sc, [1 if name in held else 0])

    def run(self, name, ticks, keys):
        m = self.m
        self.setup_look()
        self.written = set()
        rows = []
        for tick in range(ticks):
            held = keys(tick)
            self.set_look_keys(held)
            before = m.get_u32(G_POLL_INDEX)
            m.call(kb.TIMER_TICK)
            index = m.get_u32(G_POLL_INDEX)
            # Main loop: every new poll index, in order.
            for n in range(before + 1, index + 1):
                m.put_u32(G_MAIN_INDEX, n)
                row = [('run', name), ('tick', tick), ('poll', n)]
                row += [(k, int(k in held)) for k in SC]
                row.append(('word', m.get_u32(G_POLL_WORDS + 4 * (n & 0x1f))))
                for bit, col in LEVEL_BITS.items():
                    row.append((col, m.call(INPUT_BIT, bit, 0)))
                for bit, col in EDGE_BITS.items():
                    row.append((col, m.call(INPUT_BIT, bit, 1)))
                rows.append(row)
        bad = self.written - self.allowed
        if bad:
            raise RuntimeError(f'{name}: unexpected writes {sorted(hex(a) for a in bad)[:20]}')
        return rows


def record(game_dir):
    rec = LookRecorder(game_dir)
    rows = []
    for sc in scenarios():
        rows += rec.run(*sc)
    return rows, dict(rec.m.calls)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--game-dir', type=Path, default=vh.ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    first, calls = record(args.game_dir)
    second, _ = record(args.game_dir)
    if first != second:
        raise RuntimeError('two runs differ')
    size = kb.step.write_tsv(args.out_dir / 'camera-look-input.tsv', first, [
        'Look-behind / cycle-camera input: per poll the held keys (B = I_LOOKBEHIND, C = I_CYCLECAMERA),',
        'the interface word 0x5e954c and 0x4138d0 (level bits 0x13..0x16, edge bit 0x17);',
        'generated by scripts/research/replay-look-input.py (x87 control word 0x27f).'])
    print('calls:', ', '.join(f'{t:#x} x{n}' for t, n in sorted(calls.items())))
    print(f'{len(first)} polls, {size} bytes; two runs identical -> {args.out_dir}')


if __name__ == '__main__':
    main()
