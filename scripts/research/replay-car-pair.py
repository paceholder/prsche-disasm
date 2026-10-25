"""Record original car-to-car collision outputs by executing nfs5.exe x86 code.

Runs CarPair_SeparateOverlap (0x497b10, which calls CarPair_FindContact
0x496fd0), CarPair_ApplyImpulse (0x496860, including its damage callees) and
Car_ClampAngularVelocity (0x4964d0) in Unicorn on synthetic car pairs, and
writes TSV fixtures (inputs + original outputs) for the Rust regression tests.
No original instruction is patched. Field meanings: docs/re/collision.md in the
workspace root.

    python3 scripts/research/replay-car-pair.py --module nfs5.exe \
        --game-dir local/game --out-dir <crate>/tests/fixtures
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct

from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_INVALID
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_FPCW

ROOT = Path(__file__).resolve().parents[2]
CAR_A, CAR_B, RES, POINT, NORMAL = 0x2000000, 0x2004000, 0x2008000, 0x2009000, 0x2009100
STACK_TOP, EXIT = 0x2110000, 0x2200000
CAR_SIZE = 0x4000

SEPARATE, IMPULSE, CLAMP = 0x497b10, 0x496860, 0x4964d0
# Globals the pair path reads; set explicitly so fixtures don't depend on .data defaults.
G_PAIR_DAMP = 0x657408      # with car+0xdac: impulse x0.2
G_DAMAGE_ON = 0x657a2c      # Car_ApplyImpact damage gate
G_MODE = 0x6573e8           # Car_ApplyImpact branch selector
G_RAND = 0x5d1028           # Car_ApplyImpact LCG state

# Input fields per car: (name, offset, count)
CAR_INPUTS = [('pos', 0x330, 3), ('vel', 0x33c, 3), ('mass', 0x354, 1), ('inv_mass', 0x358, 1),
              ('speed', 0x35c, 1), ('axis_r', 0x364, 3), ('axis_u', 0x370, 3), ('axis_f', 0x37c, 3),
              ('ang', 0x388, 3), ('inertia', 0x394, 1), ('inv_inertia', 0x398, 1),
              ('ext', 0x3a8, 3), ('radius', 0x3b4, 1)]
CAR_OUTPUTS = [('pos', 0x330, 3), ('vel', 0x33c, 3), ('ang', 0x388, 3), ('impact', 0x430, 1),
               ('contact_pos', 0x440, 3), ('damage', 0x4e4, 9)]


# The real game runs with x87 control word 0x27f (53-bit precision; the CRT calls
# _controlfp(_PC_53)); Unicorn starts at 0 (24-bit), which rounds intermediates differently.
DEFAULT_FPCW = 0x27f


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def load_module(module_name, game_dir):
    modules = [json.loads(line) for line in
               (ROOT / 'research/binary-index/static/binaries.jsonl').read_text().splitlines()]
    module, = [m for m in modules if m['file'] == module_name]
    data = (game_dir / module['path']).read_bytes()
    if hashlib.sha256(data).hexdigest() != module['sha256']:
        raise RuntimeError('original SHA256 mismatch')
    return module, data


class Machine:
    def __init__(self, module, data, fpcw=DEFAULT_FPCW):
        self.fpcw = fpcw
        self.uc = uc = Uc(UC_ARCH_X86, UC_MODE_32)
        base = int(module['image_base'], 16)
        sections = module['sections']
        end = max(s['rva'] + max(s['vsize'], s['raw_size']) for s in sections)
        uc.mem_map(base, (end + 0xfff) & ~0xfff)
        for s in sections:
            uc.mem_write(base + s['rva'], data[s['raw_offset']:s['raw_offset'] + s['raw_size']])
        text = next(s for s in sections if s['name'] == '.text')
        self.code = (base + text['rva'], base + text['rva'] + text['vsize'])
        for addr, size in ((CAR_A, 0x10000), (STACK_TOP - 0x10000, 0x10000), (EXIT, 0x1000)):
            uc.mem_map(addr, size)
        uc.hook_add(UC_HOOK_CODE, self._guard)
        uc.hook_add(UC_HOOK_MEM_INVALID, self._invalid)
        self.snapshot = None

    def _guard(self, _uc, address, _size, _user):
        if not (self.code[0] <= address < self.code[1] or address == EXIT):
            raise RuntimeError(f'execution left .text: {address:08x}')

    @staticmethod
    def _invalid(_uc, access, address, size, _value, _user):
        raise RuntimeError(f'invalid memory access type {access} at {address:08x} size {size}')

    def put(self, addr, *values):
        self.uc.mem_write(addr, struct.pack('<' + 'f' * len(values), *values))

    def get(self, addr, count=1):
        return list(struct.unpack('<' + 'f' * count, self.uc.mem_read(addr, 4 * count)))

    def put_u32(self, addr, value):
        self.uc.mem_write(addr, struct.pack('<I', value))

    def get_u32(self, addr):
        return struct.unpack('<I', self.uc.mem_read(addr, 4))[0]

    def call(self, entry, *args):
        sp = STACK_TOP - 0x100
        frame = struct.pack('<I', EXIT) + b''.join(struct.pack('<I', a) for a in args)
        self.uc.mem_write(sp, frame)
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        self.uc.reg_write(UC_X86_REG_FPCW, self.fpcw)
        try:
            self.uc.emu_start(entry, EXIT, count=200000)
        except UcError as error:
            raise RuntimeError(f'emulation failed at {self.uc.reg_read(UC_X86_REG_EIP):08x}: {error}')
        if self.uc.reg_read(UC_X86_REG_EIP) != EXIT:
            raise RuntimeError('instruction budget exhausted')
        return self.uc.reg_read(UC_X86_REG_EAX)


def yaw_axes(yaw):
    c, s = math.cos(yaw), math.sin(yaw)
    return [c, 0., -s], [0., 1., 0.], [s, 0., c]


def make_car(pos, yaw, vel, ang=(0., 0., 0.), ext=(0.9, 0.65, 2.1), mass=1.25):
    r, u, f = yaw_axes(yaw)
    ext = [f32(e) for e in ext]
    return {'pos': list(pos), 'vel': list(vel), 'mass': mass, 'inv_mass': 1 / mass,
            'speed': max(abs(vel[0]), abs(vel[2])) + .25 * min(abs(vel[0]), abs(vel[2])),
            'axis_r': r, 'axis_u': u, 'axis_f': f, 'ang': list(ang),
            'inertia': 6 * mass, 'inv_inertia': 1 / (6 * mass), 'ext': ext,
            'radius': math.sqrt(sum(e * e for e in ext))}


def write_car(m, base, car):
    m.uc.mem_write(base, bytes(CAR_SIZE))
    for name, off, count in CAR_INPUTS:
        values = car[name] if count > 1 else [car[name]]
        m.put(base + off, *map(f32, values))
    m.uc.mem_write(base + 0x7c, b'\x01')          # physically active
    m.put_u32(base + 0x558, RES)                   # car resource (+0x518 = 0: normal box)


def read_car(m, base):
    out = {}
    for name, off, count in CAR_OUTPUTS:
        out[name] = m.get(base + off, count)
    out['contacts'] = struct.unpack('<h', m.uc.mem_read(base + 0x418, 2))[0]
    out['code'] = m.get_u32(base + 0x438)
    out['other'] = {0: 0, CAR_A: 1, CAR_B: 2}.get(m.get_u32(base + 0x434), -1)
    return out


def setup(m, a, b, damage):
    m.uc.mem_write(RES, bytes(0x1000))
    write_car(m, CAR_A, a)
    write_car(m, CAR_B, b)
    m.put(POINT, 0., 0., 0.)
    m.put(NORMAL, 0., 0., 0.)
    m.put_u32(G_PAIR_DAMP, 0)
    m.put_u32(G_DAMAGE_ON, int(damage))
    m.put_u32(G_MODE, 0)
    m.put_u32(G_RAND, 0x1234)


def sat_overlap(a, b):
    """Plan-view box overlap (both upright), to label point-sampling misses."""
    axes = [a['axis_r'], a['axis_f'], b['axis_r'], b['axis_f']]
    d = [b['pos'][i] - a['pos'][i] for i in range(3)]
    for ax in axes:
        dot = lambda v: v[0] * ax[0] + v[2] * ax[2]
        ra = a['ext'][0] * abs(dot(a['axis_r'])) + a['ext'][2] * abs(dot(a['axis_f']))
        rb = b['ext'][0] * abs(dot(b['axis_r'])) + b['ext'][2] * abs(dot(b['axis_f']))
        if abs(dot(d)) > ra + rb:
            return False
    return abs(d[1]) <= a['ext'][1] + b['ext'][1]


def named_cases():
    return [
        ('face_rear_push', make_car([0., .65, 4.1], 0., [0., 0., -20.]), make_car([0., .65, 0.], 0., [0., 0., -15.])),
        ('face_head_on', make_car([0., .65, 4.0], 0., [0., 0., -15.]), make_car([0., .65, 0.], math.pi, [0., 0., 15.])),
        ('fast_head_on_rewind', make_car([0., .65, 4.0], 0., [0., 0., -40.]), make_car([0., .65, 0.], math.pi, [0., 0., 40.])),
        ('side_scrape', make_car([-1.75, .65, 0.], 0., [1., 0., -20.]), make_car([0., .65, 0.], 0., [0., 0., -20.])),
        ('glancing', make_car([-1.6, .65, 3.4], .15, [2., 0., -25.]), make_car([0., .65, 0.], 0., [0., 0., -18.])),
        ('corner_t_bone', make_car([2.5, .65, 0.], math.pi / 2, [-12., 0., 0.]), make_car([0., .65, 0.], 0., [0., 0., -10.])),
        ('corner_45', make_car([2.2, .65, 2.6], math.pi / 4, [-6., 0., -6.]), make_car([0., .65, 0.], 0., [0., 0., 0.])),
        ('heavy_vs_light', make_car([0., .65, 4.1], 0., [0., 0., -20.], mass=1.6), make_car([0., .65, 0.], 0., [0., 0., -5.], mass=.8)),
        ('spinning', make_car([-1.7, .65, 0.], 0., [3., 0., -20.], ang=(0., 1.5, 0.)), make_car([0., .65, 0.], 0., [0., 0., -20.])),
        ('separating', make_car([-1.75, .65, 0.], 0., [-3., 0., -20.]), make_car([0., .65, 0.], 0., [0., 0., -20.])),
        ('no_overlap', make_car([-3., .65, 0.], 0., [1., 0., -20.]), make_car([0., .65, 0.], 0., [0., 0., -20.])),
        # Crossing boxes with no corner or face centre inside the other: point sampling misses it.
        ('edge_miss_cross', make_car([1.5, .65, 1.0], math.pi / 2, [-5., 0., 0.], ext=(0.9, 0.65, 3.0)),
         make_car([0., .65, 0.], 0., [0., 0., 0.])),
    ]


def random_cases(rng, count):
    cases = []
    while len(cases) < count:
        yaw_a, yaw_b = rng.uniform(-math.pi, math.pi), rng.uniform(-math.pi, math.pi)
        dist, ang = rng.uniform(1.0, 4.2), rng.uniform(-math.pi, math.pi)
        a = make_car([dist * math.cos(ang), .65 + rng.uniform(-.1, .1), dist * math.sin(ang)], yaw_a,
                     [rng.uniform(-45, 45), rng.uniform(-1, 1), rng.uniform(-45, 45)],
                     ang=(rng.uniform(-.3, .3), rng.uniform(-3, 3), rng.uniform(-.3, .3)),
                     mass=rng.uniform(.9, 1.6))
        b = make_car([0., .65, 0.], yaw_b, [rng.uniform(-45, 45), 0., rng.uniform(-45, 45)],
                     ang=(0., rng.uniform(-3, 3), 0.), mass=rng.uniform(.9, 1.6))
        if sat_overlap(a, b):
            cases.append((f'random_{len(cases)}', a, b))
    return cases


def run_pair(m, name, a, b, damage):
    setup(m, a, b, damage)
    hit = m.call(SEPARATE, CAR_A, CAR_B, POINT, NORMAL)
    sep = {'return': hit, 'point': m.get(POINT, 3), 'normal': m.get(NORMAL, 3),
           'a_pos': m.get(CAR_A + 0x330, 3), 'b_pos': m.get(CAR_B + 0x330, 3)}
    imp = None
    if hit:
        applied = m.call(IMPULSE, CAR_A, CAR_B, POINT, NORMAL)
        imp = {'return': applied, 'a': read_car(m, CAR_A), 'b': read_car(m, CAR_B)}
        m.call(CLAMP, CAR_A)
        m.call(CLAMP, CAR_B)
        imp['a_ang_clamped'] = m.get(CAR_A + 0x388, 3)
        imp['b_ang_clamped'] = m.get(CAR_B + 0x388, 3)
    label = 'hit' if hit else ('edge_miss' if sat_overlap(a, b) else 'separate')
    return {'name': name, 'damage': damage, 'label': label, 'a': a, 'b': b, 'separate': sep, 'impulse': imp}


def fmt(x):
    return repr(float(x)) if isinstance(x, float) else str(x)


def flat(prefix, value):
    if isinstance(value, dict):
        return [c for k, v in value.items() for c in flat(f'{prefix}_{k}' if prefix else k, v)]
    if isinstance(value, list):
        return [(f'{prefix}_{i}', v) for i, v in enumerate(value)]
    return [(prefix, value)]


def write_tsv(path, rows):
    header = [k for k, _ in rows[0]]
    lines = ['\t'.join(header)]
    for row in rows:
        if [k for k, _ in row] != header:
            raise RuntimeError(f'column mismatch in {path}')
        lines.append('\t'.join(fmt(v) for _, v in row))
    path.write_text('\n'.join(lines) + '\n', encoding='utf-8')


def car_in(prefix, car):
    return [(f'{prefix}_{k}', v) for name, _, count in CAR_INPUTS
            for k, v in (flat(name, car[name]) if count > 1 else [(name, car[name])])]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--module', default='nfs5.exe', choices=['Porsche.exe', 'nfs5.exe'])
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, required=True)
    parser.add_argument('--random', type=int, default=200)
    parser.add_argument('--fpcw', type=lambda v: int(v, 0), default=DEFAULT_FPCW, help='x87 control word (default 0x27f)')
    args = parser.parse_args()
    module, data = load_module(args.module, args.game_dir)
    m = Machine(module, data, args.fpcw)
    rng = random.Random(4980)
    cases = named_cases() + random_cases(rng, args.random)
    results = [run_pair(m, name, a, b, damage) for name, a, b in cases for damage in (0, 1)]

    sep_rows, imp_rows = [], []
    for r in results:
        base = [('name', r['name']), ('damage', r['damage']), ('label', r['label'])]
        inputs = car_in('a', r['a']) + car_in('b', r['b'])
        s = r['separate']
        sep_rows.append(base + inputs + [('hit', s['return'])] + flat('point', s['point']) +
                        flat('normal', s['normal']) + flat('a_pos_out', s['a_pos']) + flat('b_pos_out', s['b_pos']))
        if r['impulse']:
            i = r['impulse']
            # Impulse inputs are the post-separation state: separated positions plus p, n.
            imp_rows.append(base + inputs + flat('a_pos_sep', s['a_pos']) + flat('b_pos_sep', s['b_pos']) +
                            flat('point', s['point']) + flat('normal', s['normal']) +
                            [('applied', i['return'])] + flat('a_out', i['a']) + flat('b_out', i['b']) +
                            flat('a_ang_clamped', i['a_ang_clamped']) + flat('b_ang_clamped', i['b_ang_clamped']))
    args.out_dir.mkdir(parents=True, exist_ok=True)
    write_tsv(args.out_dir / 'car-pair-separate.tsv', sep_rows)
    write_tsv(args.out_dir / 'car-pair-impulse.tsv', imp_rows)
    labels = {}
    for r in results:
        labels[r['label']] = labels.get(r['label'], 0) + 1
    print(f"{len(results)} original runs ({module['file']}): {labels}; "
          f"{len(imp_rows)} impulse rows -> {args.out_dir}")


if __name__ == '__main__':
    main()
