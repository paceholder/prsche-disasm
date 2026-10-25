"""Record the original tyre-squeal and surface-rumble sound logic by executing nfs5.exe x86 code.

Runs unchanged in Unicorn, on a synthetic player car:
  - Car_SkidSoundEvents 0x40fbc0 (wheel skids w+0xb8 -> queue events at car+0x1110, bits at
    car+0xe00), followed by the stop of the body scrape from 0x40fff0 (not run: the roof branch
    needs the LCG);
  - the event dispatch of CarSound_UpdateCar 0x4a7660 (for each queue entry from the last to the
    first: types 3 / 5 -> Sfx_Stop 0x4a9890 (+4 for the front pair), else Sfx_PlayEvent 0x4a93c0
    with ftol(volume * 65535)), written here in Python because the function is 1.9 KB of
    listener maths;
  - the rpm term of 0x4a7660 (0x4a79b7..0x4a7a0c, executed as a range);
  - CarSound_Update 0x4aa050 (surface rumble, effect 0x18).
Sfx_PlayEvent, Sfx_Play 0x4a9c10 and 0x4a98e0 run unmodified. Audio-library entries and the
engine producer 0x4ac780 are answered by a fake and recorded. Field meanings: docs/re/audio.md
"Tyre and surface sounds" in the workspace root.

    python3 scripts/research/replay-tyre-audio.py --game-dir local/game \
        --out-dir iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures

Writes tyre-audio-steps.tsv: one row per tick (inputs, the per-channel voice state after the
tick, the events and the stops of the tick).
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_INVALID
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESI, UC_X86_REG_ESP, UC_X86_REG_EIP

ROOT = Path(__file__).resolve().parents[2]
CAR, SPECS, RES = 0x2000000, 0x2004000, 0x2005000
SCRATCH = 0x2006000
STACK_TOP, EXIT = 0x2110000, 0x2200000

SKID_EVENTS, SFX_STOP, SFX_EVENT, SFX_PLAY, CAR_SOUND = 0x40fbc0, 0x4a9890, 0x4a93c0, 0x4a9c10, 0x4aa050
RPM_TERM = (0x4a79b7, 0x4a7a0c)
G_HANDLE, G_SLOT = 0x656180, 0x656184         # [id * 2] voice handle / playing slot of effect id
G_PLAYER_CARS = 0x5e8aec                      # [id] car pointer used by 0x4a93c0 (id 0x12 -> 0x5e8b34)
G_SMOOTH = 0x6563c8                           # smoothed throttle per car
G_CAMERA, G_CAMERA_FLAGS = 0x627f3c, 0x627f43
G_DETAIL, G_SPLIT = 0x657c98, 0x657404
G_BANKS = 0x6564a0                            # bank handles, 16 bytes per group
CHANNELS = (('front', 0x12), ('rear', 0x14), ('rumble', 0x18))

# addr -> (name, bytes popped by the callee, return value)
HOOKS = {
    0x565e70: ('play', 0), 0x565dd0: ('stop_fade', 0), 0x566380: ('is_finished', 0),
    0x566340: ('set_volume', 0), 0x5661f0: ('set_pan', 0), 0x566100: ('set_pan_b', 0),
    0x5660c0: ('set_control', 0), 0x565f80: ('set_pitch', 0), 0x565f00: ('set_filter', 0),
    0x565cd0: ('ignore', 0), 0x4ac780: ('ignore', 0), 0x4ad640: ('aux', 0),
    0x473330: ('ignore', 8), 0x443ee0: ('ignore', 0), 0x516950: ('ignore', 0),
    0x538040: ('ignore', 0),
}
ARGC = {'play': 3, 'stop_fade': 3, 'is_finished': 1, 'set_volume': 2, 'set_pan': 3,
        'set_pan_b': 2, 'set_control': 2, 'set_pitch': 2, 'set_filter': 2}


def load_module(module_name, game_dir):
    modules = [json.loads(line) for line in
               (ROOT / 'research/binary-index/static/binaries.jsonl').read_text().splitlines()]
    module, = [m for m in modules if m['file'] == module_name]
    data = (game_dir / module['path']).read_bytes()
    if hashlib.sha256(data).hexdigest() != module['sha256']:
        raise RuntimeError('original SHA256 mismatch')
    return module, data


def s32(x):
    return x - (1 << 32) if x & 0x80000000 else x


class FakeAudio:
    """Handles from 100; a stopped voice is gone at once (the fade is only recorded)."""

    def __init__(self):
        self.next_handle = 100
        self.voices = {}
        self.stops = []

    def call(self, name, args, params):
        if name == 'play':
            h = self.next_handle
            self.next_handle += 1
            self.voices[h] = {'bank': args[0], 'slot': args[1], 'vol': params[8], 'ctl': params[9],
                              'pitch': struct.unpack_from('<H', params, 0xc)[0], 'pan': params[7],
                              'loop': params[0xb], 'filter': struct.unpack_from('<H', params, 0x12)[0]}
            return h
        v = self.voices.get(args[0]) if args else None
        if name == 'is_finished':
            return 0 if v else 1
        if name == 'stop_fade':
            if v:
                self.stops.append((v['slot'], args[1]))
                del self.voices[args[0]]
            return args[0]
        if v is None:
            return 0
        if name == 'set_volume':
            v['vol'] = args[1]
        elif name == 'set_control':
            v['ctl'] = args[1]
        elif name == 'set_pitch':
            v['pitch'] = args[1] & 0xffff
        elif name == 'set_filter':
            v['filter'] = args[1]
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
        for addr, size in ((CAR, 0x8000), (STACK_TOP - 0x10000, 0x10000), (EXIT, 0x1000)):
            uc.mem_map(addr, size)
        uc.hook_add(UC_HOOK_CODE, self._hook)
        uc.hook_add(UC_HOOK_MEM_INVALID, self._invalid)
        self.audio = FakeAudio()
        self.engine_calls = 0

    def _hook(self, uc, address, _size, _user):
        if address in HOOKS:
            name, pop = HOOKS[address]
            sp = uc.reg_read(UC_X86_REG_ESP)
            argc = ARGC.get(name, 0)
            ret, *args = struct.unpack('<' + 'I' * (1 + argc), uc.mem_read(sp, 4 * (1 + argc)))
            args = [s32(a) for a in args]
            result = 0
            if name == 'play':
                result = self.audio.call(name, args, bytes(uc.mem_read(args[2] & 0xffffffff, 0x1c)))
            elif name == 'aux':
                result = 100
            elif name != 'ignore':
                result = self.audio.call(name, args, None)
            uc.reg_write(UC_X86_REG_EAX, result & 0xffffffff)
            uc.reg_write(UC_X86_REG_ESP, sp + 4 + pop)
            uc.reg_write(UC_X86_REG_EIP, ret)
            return
        if not (self.code[0] <= address < self.code[1] or address == EXIT):
            raise RuntimeError(f'execution left .text: {address:08x}')

    @staticmethod
    def _invalid(_uc, access, address, size, _value, _user):
        raise RuntimeError(f'invalid memory access type {access} at {address:08x} size {size}')

    def i32(self, addr, count=1):
        return list(struct.unpack('<' + 'i' * count, self.uc.mem_read(addr, 4 * count)))

    def f32(self, addr):
        return struct.unpack('<f', self.uc.mem_read(addr, 4))[0]

    def put_i32(self, addr, *values):
        self.uc.mem_write(addr, struct.pack('<' + 'i' * len(values), *values))

    def put_f32(self, addr, value):
        self.uc.mem_write(addr, struct.pack('<f', value))

    def call(self, entry, *args, ecx=None):
        sp = STACK_TOP - 0x100
        frame = struct.pack('<I', EXIT) + b''.join(struct.pack('<I', a & 0xffffffff) for a in args)
        self.uc.mem_write(sp, frame)
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        if ecx is not None:
            self.uc.reg_write(UC_X86_REG_ECX, ecx)
        try:
            self.uc.emu_start(entry, EXIT, count=400000)
        except UcError as error:
            raise RuntimeError(f'emulation failed at {self.uc.reg_read(UC_X86_REG_EIP):08x}: {error}')
        if self.uc.reg_read(UC_X86_REG_EIP) != EXIT:
            raise RuntimeError('instruction budget exhausted')
        return self.uc.reg_read(UC_X86_REG_EAX)

    def run_range(self, begin, until, regs):
        """Executes begin..until on the current frame (a scratch stack)."""
        sp = STACK_TOP - 0x400
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        for reg, value in regs.items():
            self.uc.reg_write(reg, value)
        self.uc.emu_start(begin, until, count=400000)
        return self.uc.reg_read(UC_X86_REG_EAX)


def put_car(m, c):
    """Writes the car fields the chain reads (offsets: docs/re/audio.md)."""
    w = lambda off, *v: m.put_i32(CAR + off, *v)
    wf = lambda off, v: m.put_f32(CAR + off, v)
    wf(0x870, c['f0']); wf(0x934, c['f1']); wf(0x9f8, c['r0']); wf(0xabc, c['r1'])
    wf(0x78, c['f78']); wf(0x424, c['height']); wf(0x3a0, c['up'])
    w(0x464, c['surface']); w(0x540, c['o540'])
    w(0x52c, 4 if c['bit4'] else 0)
    w(0x760, SPECS); w(0x558, RES)
    m.put_i32(SPECS + 0x1c0, c['tyre'])
    m.put_f32(SPECS + 0x94, c['redline']); m.put_i32(SPECS + 0x54, 5)
    m.put_i32(RES + 0xc, c['resc']); m.put_i32(RES + 0x518, 0)
    m.put_f32(CAR + 0xd60, c['d60'])
    wf(0x35c, abs(c['d60'])); wf(0xdb4, c['rpm']); wf(0x54c, 0.0)
    w(0x520, 0); w(0xdb0, 1); w(0x548, 0); w(0xd98, 0)
    m.uc.mem_write(CAR + 0xd7f, bytes([c['gas']])); m.uc.mem_write(CAR + 0xd80, bytes([0]))
    m.uc.mem_write(CAR + 0xd84, bytes([0])); m.uc.mem_write(CAR + 0xd81, bytes([3]))
    m.uc.mem_write(CAR + 0xd82, bytes([3]))
    m.uc.mem_write(CAR + 0x418, struct.pack('<H', c['off']))
    m.uc.mem_write(CAR + 0xe00, bytes([c['bits']]))


def reset(m):
    m.audio = FakeAudio()
    m.uc.mem_write(CAR, bytes(0x8000))
    for i in range(0x80):
        m.put_i32(G_HANDLE + i * 8, -1, -1)
    m.put_i32(G_PLAYER_CARS + 0x12 * 4, CAR)
    m.put_i32(G_PLAYER_CARS + 0x13 * 4, CAR)
    for g in range(8):
        m.put_i32(G_BANKS + g * 16, 40 + g)
    m.put_i32(G_SMOOTH, 0, 0)
    m.put_i32(0x656860, 0)
    m.put_i32(0x656864, 0)
    m.uc.mem_write(0x656868, bytes([0, 0]))
    m.put_i32(G_DETAIL, 0)
    m.put_i32(G_SPLIT, 0)
    m.put_i32(0x606ae0, 0)
    m.put_i32(0x655d40, 0)


def tick(m, c, state):
    """One effects tick + audio update. Returns the row of results."""
    m.audio.stops = []
    m.put_i32(G_CAMERA, c['camera'])
    m.uc.mem_write(G_CAMERA, struct.pack('<H', c['camera']))
    m.uc.mem_write(G_CAMERA_FLAGS, bytes([0]))
    put_car(m, dict(c, bits=state['bits']))
    m.put_i32(CAR + 0x110c, 0)
    m.call(SKID_EVENTS, CAR)
    count = m.i32(CAR + 0x110c)[0]
    state['bits'] = m.uc.mem_read(CAR + 0xe00, 1)[0]
    events = []
    for i in range(count):
        t, ident, tyre, patch = m.i32(CAR + 0x1110 + 0x18 * i, 4)
        vol = m.f32(CAR + 0x1120 + 0x18 * i)
        events.append((t, ident, tyre, patch, vol))
    for t, ident, tyre, patch, vol in reversed(events):       # 0x4a7660 walks the queue backwards
        if t in (3, 5) and ident > -1:
            m.call(SFX_STOP, ident)
            if ident in (0x12, 0x13):
                m.call(SFX_STOP, ident + 4)
        else:
            p4 = int(vol * 65535.0)
            m.call(SFX_EVENT, ident, tyre, patch, p4, 0, 0)
    # rpm term of 0x4a7660 (x87), then CarSound_Update with the listener at the car
    m.uc.mem_write(SCRATCH, bytes(0x100))
    m.uc.mem_write(STACK_TOP - 0x400 + 0x28, struct.pack('<i', 0))
    m.run_range(RPM_TERM[0], RPM_TERM[1] + 4, {UC_X86_REG_ESI: CAR})
    p_term = struct.unpack('<i', m.uc.mem_read(STACK_TOP - 0x400 + 0x28, 4))[0]
    m.call(CAR_SOUND, CAR, 0, p_term, 0x10000, 0, 0, 0, 0)
    row = {'events': ';'.join(f'{t}:{i:#x}:{ty}:{pa}:{v:.9g}' for t, i, ty, pa, v in events),
           'p_term': p_term, 'bits_out': state['bits'],
           'stops': ';'.join(f'{s}:{t}' for s, t in sorted(m.audio.stops))}
    for name, ident in CHANNELS:
        h, slot = m.i32(G_HANDLE + ident * 8, 2)
        v = m.audio.voices.get(h) if h != -1 else None
        row[f'{name}_slot'] = v['slot'] if v else -1
        row[f'{name}_vol'] = v['vol'] if v else -1
        row[f'{name}_ctl'] = v['ctl'] if v else -1
        row[f'{name}_pitch'] = v['pitch'] if v else -1
    return row


INPUTS = ['camera', 'f0', 'f1', 'r0', 'r1', 'f78', 'height', 'up', 'surface', 'o540', 'tyre',
          'bit4', 'd60', 'rpm', 'redline', 'off', 'resc', 'gas']


def car(**kw):
    c = {'camera': 2, 'f0': 0.0, 'f1': 0.0, 'r0': 0.0, 'r1': 0.0, 'f78': 0.0, 'height': 0.0,
         'up': 1.0, 'surface': 1, 'o540': 0, 'tyre': 0, 'bit4': 0, 'd60': 30.0, 'rpm': 4000.0,
         'redline': 7000.0, 'off': 0, 'resc': 0, 'gas': 0}
    c.update(kw)
    return c


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def cases():
    out = []
    ramp = [f32(i / 40) for i in range(0, 41)]
    out.append(('front_ramp_road', [car(f0=v, f1=v) for v in ramp + ramp[::-1]]))
    for tyre in (1, 2):
        out.append((f'front_ramp_tyre{tyre}', [car(f0=v, f1=v * 0.5, tyre=tyre) for v in ramp + ramp[::-1]]))
    out.append(('rear_ramp', [car(r0=v, r1=v * 0.8) for v in ramp + ramp[::-1]]))
    out.append(('both_axles', [car(f0=v, f1=0.1, r0=v, r1=0.3) for v in ramp + [f32(2.5), f32(6), f32(12)]]))
    for s in range(16):
        out.append((f'surface_{s}', [car(surface=s, f0=0.05 * k, f1=0.05 * k, r0=0.2, r1=0.1, d60=12.0 + 2 * k)
                                      for k in range(12)]))
    out.append(('surface_switch', [car(surface=(k // 6) % 16, f0=0.4, f1=0.3, d60=25.0) for k in range(100)]))
    out.append(('speed_road_chase', [car(d60=f32(k * 0.8), surface=1, camera=2) for k in range(80)]))
    out.append(('speed_road_bumper', [car(d60=f32(k * 0.8), surface=1, camera=0) for k in range(80)]))
    out.append(('speed_grass_chase', [car(d60=f32(k * 0.4), surface=4, camera=2) for k in range(80)]))
    out.append(('speed_grass_bumper', [car(d60=f32(k * 0.4), surface=4, camera=0) for k in range(80)]))
    out.append(('speed_reverse_free', [car(d60=f32(-k * 0.5), surface=6, camera=5) for k in range(60)]))
    out.append(('rpm_sweep', [car(rpm=f32(500 + 80 * k), d60=14.0, surface=2, resc=k % 2, off=(k // 7) % 2 * 9)
                              for k in range(100)]))
    out.append(('redline_values', [car(redline=r, rpm=0.9 * r, d60=20.0, surface=3) for r in (3000.0, 6500.0, 8200.0, 9500.0)]))
    out.append(('gates', [car(f0=0.5, f1=0.5, r0=0.5, r1=0.5, **kw) for kw in (
        {}, {'height': 0.05}, {'height': 0.1}, {'height': 0.2}, {}, {'up': 0.5}, {'up': 0.51}, {},
        {'f78': 59.0}, {'f78': 60.0}, {}, {'o540': 1}, {'o540': 1, 'surface': 6}, {})]))
    out.append(('bit4_hold', [car(bit4=1, f0=v, f1=0.0) for v in (0.0, 0.0, 0.3, 0.0, 0.0, 0.3, 0.0)]))
    out.append(('skid_values', [car(f0=a, f1=b, r0=c, r1=d) for a, b, c, d in (
        (0.0, 0.004, 0.0, 0.0), (0.004, 0.004, 0.003, 0.0), (0.12, 0.0, 0.0, 0.0), (0.126, 0.0, 0.0, 0.0),
        (6.0, 5.0, 11.0, 12.0), (-0.5, 0.2, -0.3, 0.1), (0.0, 0.0, 0.0, 0.0))]))
    return out


def write_tsv(path, rows):
    header = list(rows[0].keys())
    lines = ['\t'.join(header)]
    for row in rows:
        lines.append('\t'.join(str(row[k]) for k in header))
    path.write_text('\n'.join(lines) + '\n', encoding='utf-8')


def fmt(v):
    return repr(v) if isinstance(v, float) else v


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--module', default='nfs5.exe', choices=['Porsche.exe', 'nfs5.exe'])
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    module, data = load_module(args.module, args.game_dir)
    m = Machine(module, data)
    rows = []
    for name, steps in cases():
        reset(m)
        state = {'bits': 0}
        for step, c in enumerate(steps):
            r = tick(m, c, state)
            rows.append({'case': name, 'step': step, **{k: fmt(c[k]) for k in INPUTS}, **r})
    args.out_dir.mkdir(parents=True, exist_ok=True)
    write_tsv(args.out_dir / 'tyre-audio-steps.tsv', rows)
    print(f"{len(rows)} ticks ({module['file']}) -> {args.out_dir}")


if __name__ == '__main__':
    main()
