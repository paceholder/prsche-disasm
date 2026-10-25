"""Record original engine-audio producer and mixer outputs by executing nfs5.exe x86 code.

Runs EngineAudio_PushState (0x4ac780) and EngineAudio_Mix (0x4ac290) unchanged in
Unicorn on synthetic engine-audio states built the way EngineAudio_Load (0x4acc10)
builds them: its defaults, the car's `.spl` over the first 13 fields and the four
CRDl tables (.ect/.elt/.cct/.clt) relocated and copied into an "Engine Tables"
block. Audio-library entry points are not executed: each call is recorded
(call site, function, arguments, play parameters), answered with a fake value
(voice handle, finished flag, remaining time of a modelled one-shot) and resumed
at its return address. Audio_InitPlayParams (0x561960) is a pure struct fill and
runs unchanged. No original instruction is patched. Field meanings:
docs/re/audio.md in the workspace root.

    python3 scripts/research/replay-engine-audio.py --game-dir local/game \
        --out-dir local/fixtures

Writes engine-audio-steps.tsv (one row per producer call = one mixer update) and
engine-audio-calls.tsv (one row per recorded audio-library call).
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_INVALID
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBX, UC_X86_REG_EDI, UC_X86_REG_ESP,
                               UC_X86_REG_EIP)

ROOT = Path(__file__).resolve().parents[2]
STATE, TABLES = 0x2000000, 0x2001000
STACK_TOP, EXIT = 0x2110000, 0x2200000
STATE_SIZE = 0x758

PUSH, MIX = 0x4ac780, 0x4ac290
PUSH_END = 0x4acb7f
GAIN_APPLY = 0x4acb54        # ebx = start/stop gain (0..512), [edi] = master before gain
G_ENGINE_AUDIO = 0x656aa8    # [2] state pointers; the mixer only reads [0]
G_SPREAD = 0x655f34          # spread source for modes 0, 1
G_SPLIT = 0x657404           # == 1: no spread (split screen)
G_DETAIL = 0x657c98          # audio detail; >= 2: slots < 0x40 muted, *16.viv banks
BANK_HANDLE = 3              # byte at state+0xD0 (from 0x6564a0[player])

LIB = {0x565e70: ('play', 3), 0x565d60: ('stop', 1), 0x566340: ('set_volume', 2),
       0x5661f0: ('set_pan', 3), 0x565f80: ('set_pitch', 2), 0x566380: ('is_finished', 1),
       0x566430: ('time_remaining', 1)}
RING_FIELDS = ['master', 'pitch', 'pan', 'spread', 'bin', 'group', 'load', 'mode']
TABLE_EXT = ['.ect', '.elt', '.cct', '.clt']


def load_module(module_name, game_dir):
    modules = [json.loads(line) for line in
               (ROOT / 'research/binary-index/static/binaries.jsonl').read_text().splitlines()]
    module, = [m for m in modules if m['file'] == module_name]
    data = (game_dir / module['path']).read_bytes()
    if hashlib.sha256(data).hexdigest() != module['sha256']:
        raise RuntimeError('original SHA256 mismatch')
    return module, data


def read_viv(path):
    d = path.read_bytes()
    if d[:4] != b'BIGF':
        raise RuntimeError(f'{path}: not a BIGF archive')
    count, p, files = struct.unpack_from('>I', d, 8)[0], 16, {}
    for _ in range(count):
        off, size = struct.unpack_from('>II', d, p)
        end = d.index(b'\0', p + 8)
        files[d[p + 8:end].decode('latin1')] = d[off:off + size]
        p = end + 1
    return files


def s32(x):
    return x - (1 << 32) if x & 0x80000000 else x


class FakeAudio:
    """Deterministic stand-in for the audio library: handles, one-shot timers."""

    def __init__(self, oneshot_cs):
        self.oneshot_cs = oneshot_cs   # slot -> length in 1/100 s (start 6..8, stop 9..11)
        self.next_handle = 100
        self.voices = {}               # handle -> remaining 1/100 s (None = looping)

    def advance(self, cs):
        for h, rem in self.voices.items():
            if rem is not None:
                self.voices[h] = rem - cs

    def call(self, name, args):
        if name == 'play':
            h = self.next_handle
            self.next_handle += 1
            self.voices[h] = self.oneshot_cs.get(args[1])
            return h
        if name == 'stop':
            self.voices.pop(args[0], None)
            return 0
        if name == 'is_finished':
            rem = self.voices.get(args[0], 0)
            return int(rem is not None and rem <= 0)
        if name == 'time_remaining':
            if args[0] not in self.voices:
                return -8
            rem = self.voices[args[0]]
            return 0x7fffffff if rem is None else max(rem, 0)
        return 0


class Machine:
    def __init__(self, module, data):
        self.uc = uc = Uc(UC_ARCH_X86, UC_MODE_32)
        base = int(module['image_base'], 16)
        sections = module['sections']
        end = max(s['rva'] + max(s['vsize'], s['raw_size']) for s in sections)
        uc.mem_map(base, (end + 0xfff) & ~0xfff)
        for s in sections:
            uc.mem_write(base + s['rva'], data[s['raw_offset']:s['raw_offset'] + s['raw_size']])
        text = next(s for s in sections if s['name'] == '.text')
        self.code = (base + text['rva'], base + text['rva'] + text['vsize'])
        for addr, size in ((STATE, 0x10000), (STACK_TOP - 0x10000, 0x10000), (EXIT, 0x1000)):
            uc.mem_map(addr, size)
        uc.hook_add(UC_HOOK_CODE, self._hook)
        uc.hook_add(UC_HOOK_MEM_INVALID, self._invalid)
        self.audio = None
        self.calls = []
        self.gain = None

    def _hook(self, uc, address, _size, _user):
        if address in LIB:
            name, argc = LIB[address]
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret, *args = struct.unpack('<' + 'I' * (1 + argc), uc.mem_read(sp, 4 * (1 + argc)))
            args = [s32(a) for a in args]
            params = None
            if name == 'play':
                params = bytes(uc.mem_read(args[2] & 0xffffffff, 0x1c))
            result = self.audio.call(name, args)
            self.calls.append({'caller': 'push' if PUSH <= ret <= PUSH_END else 'mix',
                               'site': ret - 5, 'func': name, 'args': args, 'ret': result,
                               'params': params})
            uc.reg_write(UC_X86_REG_EAX, result & 0xffffffff)
            uc.reg_write(UC_X86_REG_ESP, sp + 4)
            uc.reg_write(UC_X86_REG_EIP, ret)
            return
        if address == GAIN_APPLY:
            master = struct.unpack('<H', uc.mem_read(uc.reg_read(UC_X86_REG_EDI), 2))[0]
            self.gain = (s32(uc.reg_read(UC_X86_REG_EBX)), master)
        if not (self.code[0] <= address < self.code[1] or address == EXIT):
            raise RuntimeError(f'execution left .text: {address:08x}')

    @staticmethod
    def _invalid(_uc, access, address, size, _value, _user):
        raise RuntimeError(f'invalid memory access type {access} at {address:08x} size {size}')

    def u8(self, addr):
        return self.uc.mem_read(addr, 1)[0]

    def i32(self, addr, count=1):
        return list(struct.unpack('<' + 'i' * count, self.uc.mem_read(addr, 4 * count)))

    def put_i32(self, addr, *values):
        self.uc.mem_write(addr, struct.pack('<' + 'i' * len(values), *values))

    def call(self, entry, *args):
        sp = STACK_TOP - 0x100
        frame = struct.pack('<I', EXIT) + b''.join(struct.pack('<I', a & 0xffffffff) for a in args)
        self.uc.mem_write(sp, frame)
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        try:
            self.uc.emu_start(entry, EXIT, count=200000)
        except UcError as error:
            raise RuntimeError(f'emulation failed at {self.uc.reg_read(UC_X86_REG_EIP):08x}: {error}')
        if self.uc.reg_read(UC_X86_REG_EIP) != EXIT:
            raise RuntimeError('instruction budget exhausted')


def build_state(m, spl, tables, running):
    """Engine-audio state as EngineAudio_Load 0x4acc10 leaves it (player 0)."""
    st = bytearray(STATE_SIZE)
    def w32(off, v):
        struct.pack_into('<i', st, off, v)
    w32(0x34, 0)                       # tick counter
    w32(0x38, -1)                      # start voice
    w32(0x3c, -1)                      # stop voice: not set by 0x4acc10 (allocator contents), set as 0x4acb80 does
    st[0x40] = int(running)
    w32(0xc4, 0); w32(0xc8, 0); w32(0xcc, 0x1000)
    for v in range(32):
        e = 0x2d4 + 12 * v
        struct.pack_into('<hhi', st, e, 0x200, 0x200, 0)
        st[e + 8] = 0xff
        w32(0x45c + 12 * v, -1)        # first voice handle
        w32(0x5dc + 12 * v, -1)        # spread voice handle
    for i, value in enumerate([50, 0, 70, 20, 64, 128, 128, 170, 80, 104, 25, 220, 30]):
        w32(4 * i, value)
    st[0:0x34] = spl                   # File_ReadInto(path, state, 0x34)
    st[0xd0] = BANK_HANDLE
    # Table relocation and copy (0x4acc10 after the archive walk).
    block = bytearray()
    for t, tbl in enumerate(tables):
        if tbl[:4] != b'CRDl' or tbl[4] != 2:
            raise RuntimeError('bad CRDl table')
        for j in range(8):
            slot = tbl[0x20 + j]
            if slot >= 0x80:
                continue
            field = 0x128 + 4 * j
            curve = tbl[field + struct.unpack_from('<i', tbl, field)[0]:][:0x200]
            start = next((i for i in range(0x200) if curve[i]), 0x200)
            end = next((i for i in range(start, 0x200) if not curve[i]), 0x200)
            e = 0x2d4 + 12 * (8 * t + j)
            ptr = TABLES + len(block) if end > start else 0
            struct.pack_into('<hhi', st, e, start, end, ptr)
            st[e + 8] = slot
            block += curve[start:end]
    w32(0x754, TABLES)
    m.uc.mem_write(STATE, bytes(st))
    m.uc.mem_write(TABLES, bytes(block))
    m.put_i32(G_ENGINE_AUDIO, STATE, 0)


def run_case(m, case, spl, banks):
    detail = case.get('detail', 0)
    tables = banks[detail >= 2]
    m.audio = FakeAudio(case.get('oneshot_cs', {}))
    build_state(m, spl, tables, case.get('running', True))
    m.put_i32(G_SPREAD, case.get('g_spread', 0))
    m.put_i32(G_SPLIT, 0)
    m.put_i32(G_DETAIL, detail)
    rows, calls = [], []
    for step, inp in enumerate(case['steps']):
        m.audio.advance(case.get('step_cs', 0))
        m.calls, m.gain = [], None
        wr = m.u8(STATE + 0x2d2)
        m.call(PUSH, 0, inp['volume'], inp['rpm16'], inp['load'], inp['mode'], inp['doppler'],
               inp['pan'], inp['fb'])
        if m.u8(STATE + 0x2d2) != (wr + 1) & 0x1f:
            raise RuntimeError(f"{case['name']} step {step}: state not pushed")
        ring = list(struct.unpack('<8H', m.uc.mem_read(STATE + 0xd2 + 16 * wr, 16)))
        mix_calls, consumed = 1, 0
        while m.u8(STATE + 0x2d3) != m.u8(STATE + 0x2d2):
            m.call(MIX)
            mix_calls += 1
            if mix_calls > 8:
                raise RuntimeError('mixer did not consume the state')
        gain, master_pre = m.gain
        state = {'gain': gain, 'master_pre': master_pre,
                 **{f'ring_{k}': v for k, v in zip(RING_FIELDS, ring)},
                 'mix_calls': mix_calls,
                 'out_pan': m.i32(STATE + 0xc4)[0], 'out_spread': m.i32(STATE + 0xc8)[0],
                 'out_pitch': m.i32(STATE + 0xcc)[0],
                 'start_voice': m.i32(STATE + 0x38)[0], 'stop_voice': m.i32(STATE + 0x3c)[0],
                 'running': m.u8(STATE + 0x40), 'calls': len(m.calls)}
        vols = m.i32(STATE + 0x44, 32)
        base = [('case', case['name']), ('step', step), ('detail', detail),
                ('g_spread', case.get('g_spread', 0))] + [(k, inp[k]) for k in INPUTS]
        rows.append(base + list(state.items()) + [(f'vol{i}', v) for i, v in enumerate(vols)])
        for c in m.calls:
            a = c['args'] + [''] * (3 - len(c['args']))
            p = c['params']
            calls.append([('case', case['name']), ('step', step), ('caller', c['caller']),
                          ('site', f"{c['site']:#x}"), ('func', c['func']),
                          ('a0', a[0]), ('a1', a[1]), ('a2', '' if c['func'] == 'play' else a[2]),
                          ('ret', c['ret']),
                          ('p_volume', p[8] if p else ''),
                          ('p_flag', p[0xb] if p else ''),
                          ('p_pitch', struct.unpack_from('<H', p, 0xc)[0] if p else ''),
                          ('p_pan', struct.unpack_from('<h', p, 0x18)[0] if p else ''),
                          ('params_hex', p.hex() if p else '')])
    return rows, calls


INPUTS = ['volume', 'rpm16', 'load', 'mode', 'doppler', 'pan', 'fb']
VOLUME = 1016                    # listener at the car (docs/re/audio.md, volume argument)


def inp(rpm, load=0, mode=3, volume=VOLUME, doppler=0x10000, pan=0, fb=0):
    return {'volume': volume, 'rpm16': int(rpm), 'load': load, 'mode': mode, 'doppler': doppler,
            'pan': pan, 'fb': fb}


def hold(n, **kw):
    return [inp(**kw) for _ in range(n)]


def sweep(lo, hi, n, **kw):
    return [inp(lo + (hi - lo) * i // (n - 1), **kw) for i in range(n)]


def cases():
    idle, mid = 0x2400, 0x9000
    out = []
    # RPM sweeps, idle to past redline (0x10000), hold then ramp then hold.
    for name, kw in (('rpm_sweep_noload', dict(load=0, mode=3)),
                     ('rpm_sweep_fullload', dict(load=127, mode=3)),
                     ('rpm_sweep_halfload_chase', dict(load=64, mode=2)),
                     ('rpm_sweep_cockpit', dict(load=127, mode=0)),
                     ('rpm_sweep_bumper', dict(load=0, mode=1))):
        out.append({'name': name, 'steps': hold(16, rpm=idle, **kw) + sweep(idle, 0x11000, 120, **kw) +
                    hold(16, rpm=0x11000, **kw)})
    # Throttle steps at constant RPM: 0 -> full -> 0 -> half -> out of range.
    t = []
    for load, n in ((0, 20), (127, 30), (0, 30), (64, 20), (100, 10), (200, 10), (0, 20)):
        t += hold(n, rpm=mid, load=load, mode=3)
    out.append({'name': 'throttle_steps', 'steps': t})
    # View modes 0..3, with pan / front-back for modes >= 2 and spread for 0, 1.
    v = []
    for mode in (3, 0, 1, 2, 3, 0):
        v += hold(20, rpm=mid, load=64, mode=mode)
    for fb in (-0x10000, -0x8000, 0x8000, 0x10000):
        v += hold(12, rpm=mid, load=64, mode=2, pan=0x3000, fb=fb)
    v += hold(12, rpm=mid, load=64, mode=3, pan=-0x4000, fb=0x10000)
    out.append({'name': 'view_modes', 'steps': v})
    out.append({'name': 'view_modes_spread', 'g_spread': 96, 'steps': v})
    # Doppler and master-volume clamp.
    d = [inp(mid, load=64, mode=2, doppler=dop) for dop in range(0x8000, 0x18001, 0x800)]
    d += [inp(mid, load=64, mode=3, volume=vol) for vol in range(0, 4097, 128)]
    out.append({'name': 'doppler_volume', 'steps': d})
    # Low audio detail (sd1a16 tables, slots < 0x40 muted).
    out.append({'name': 'low_detail', 'detail': 2,
                'steps': sweep(idle, 0x10000, 40, load=64, mode=3) + hold(10, rpm=mid, load=64, mode=0)})
    # Start: not running; start sample 6.00 s, 8/100 s per step (gain 0 until 5.12 s remain).
    out.append({'name': 'start_chase', 'running': False, 'oneshot_cs': {8: 600}, 'step_cs': 8,
                'steps': hold(100, rpm=idle, load=0, mode=3)})
    out.append({'name': 'start_cockpit', 'running': False, 'oneshot_cs': {6: 300}, 'step_cs': 8,
                'steps': hold(60, rpm=idle, load=0, mode=0)})
    # Stop: running at mid RPM, then load -1; stop sample 3.00 s.
    out.append({'name': 'stop_chase', 'oneshot_cs': {11: 300}, 'step_cs': 8,
                'steps': hold(20, rpm=mid, load=64, mode=3) + hold(60, rpm=mid, load=-1, mode=3)})
    # Stop then restart while the stop sample plays (cancels it), then start completes.
    out.append({'name': 'stop_restart_bumper', 'oneshot_cs': {7: 200, 10: 300}, 'step_cs': 8,
                'steps': hold(10, rpm=mid, load=0, mode=1) + hold(15, rpm=mid, load=-1, mode=1) +
                hold(40, rpm=idle, load=0, mode=1)})
    return out


def fmt(x):
    return str(x)


def write_tsv(path, rows):
    header = [k for k, _ in rows[0]]
    lines = ['\t'.join(header)]
    for row in rows:
        if [k for k, _ in row] != header:
            raise RuntimeError(f'column mismatch in {path}')
        lines.append('\t'.join(fmt(v) for _, v in row))
    path.write_text('\n'.join(lines) + '\n', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--module', default='nfs5.exe', choices=['Porsche.exe', 'nfs5.exe'])
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--bank', default='sd1a')
    parser.add_argument('--spl', default='356Acabrio16')
    args = parser.parse_args()
    module, data = load_module(args.module, args.game_dir)
    m = Machine(module, data)
    sounds = args.game_dir / 'GameData/Sounds'
    spl = (sounds / f'{args.spl}.spl').read_bytes()
    if len(spl) != 0x34:
        raise RuntimeError('.spl must be 52 bytes')
    banks = []
    for suffix in ('', '16'):
        # The loader matches the last four characters of each archive name.
        files = {k[-4:].lower(): v for k, v in read_viv(sounds / f'{args.bank}{suffix}.viv').items()}
        banks.append([files[ext] for ext in TABLE_EXT])
    rows, calls = [], []
    for case in cases():
        r, c = run_case(m, case, spl, banks)
        rows += r
        calls += c
    args.out_dir.mkdir(parents=True, exist_ok=True)
    write_tsv(args.out_dir / 'engine-audio-steps.tsv', rows)
    write_tsv(args.out_dir / 'engine-audio-calls.tsv', calls)
    print(f"{len(rows)} producer/mixer updates, {len(calls)} library calls "
          f"({module['file']}, {args.bank}, {args.spl}.spl) -> {args.out_dir}")


if __name__ == '__main__':
    main()
