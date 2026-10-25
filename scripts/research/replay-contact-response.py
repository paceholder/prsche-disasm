"""Differential probe of an original contact-response slice, not a game solver.

Install unicorn==2.1.3 into local/tools/python-unicorn (or run with a Python that
has it). --module selects Porsche.exe or nfs5.exe; their code bytes match. Runs original x86 bytes
from 0x4940ae after geometry preparation, including the optional angular callback.
No original instruction is patched or replaced. See original-collision/response.md.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'local/tools/python-unicorn'))
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EBX, UC_X86_REG_EDI, UC_X86_REG_ESP, UC_X86_REG_EIP, UC_X86_REG_FPCW

CAR, NORMAL, STACK, EXIT, RESULT = 0x2000000, 0x2002000, 0x2108000, 0x2200000, 0x2200100
WHEELS = (0x7f8, 0x8bc, 0x980, 0xa44)


# The real game runs with x87 control word 0x27f (53-bit precision; the CRT calls
# _controlfp(_PC_53)); Unicorn starts at 0 (24-bit), which rounds intermediates differently.
FPCW = 0x27f


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def approximate_speed(v):
    return f32(max(abs(v[0]), abs(v[2])) + .25 * min(abs(v[0]), abs(v[2])))


def angular_response(strength, old_forward, normal, old_angular):
    # 0x493f10: gate uses the OLD projection, before 0x493e40 refreshes it.
    if strength >= 15 and old_forward >= 20:
        return old_angular
    a, b = normal[0], -normal[2]  # dot with fixture car+4a8 / car+490
    value = min(abs(a), abs(b)) * .25
    if a < 0:
        if b < 0:
            value = -value
    else:
        if b > 0:
            value = -value
        value *= .5
    return f32(value)


def machine(module, data, extra_ranges=()):
    uc = Uc(UC_ARCH_X86, UC_MODE_32)
    base = int(module['image_base'], 16)
    end = max(s['rva'] + max(s['vsize'], s['raw_size']) for s in module['sections'])
    uc.mem_map(base, (end + 4095) & ~4095)
    for section in module['sections']:
        start = section['raw_offset']
        uc.mem_write(base + section['rva'], data[start:start + section['raw_size']])
    uc.mem_map(CAR, 0x4000)
    uc.mem_map(0x2100000, 0x10000)
    uc.mem_map(EXIT, 0x1000)
    # Harness only: store the returned x87 float. Original bytes stay intact.
    uc.mem_write(EXIT, b'\xd9\x1d' + struct.pack('<I', RESULT))
    allowed = ((0x4940ae, 0x4943a0), (0x493dd0, 0x493f01), (0x493f10, 0x493ff4),
               (0x532360, 0x532397), (0x532910, 0x532923),
               (0x56e8d1, 0x56e8f5), (0x516950, 0x516951), (EXIT, EXIT + 6)) + extra_ranges

    def guard(_uc, address, _size, _user):
        if not any(lo <= address < hi for lo, hi in allowed):
            raise RuntimeError(f'unexpected original path: {address:08x}')
    uc.hook_add(UC_HOOK_CODE, guard)
    return uc


def run_case(module, data, fixture):
    uc = machine(module, data)

    def put(address, *values):
        uc.mem_write(address, struct.pack('<' + 'f' * len(values), *values))

    def get(address, count=1):
        return list(struct.unpack('<' + 'f' * count, uc.mem_read(address, count * 4)))

    velocity, normal, correction = (fixture[k] for k in ('velocity', 'normal', 'correction'))
    position = [10., 20., 30.]
    put(CAR + 0x330, *position)
    put(CAR + 0x33c, *velocity)
    speed = approximate_speed(velocity)
    put(CAR + 0x35c, speed)
    for offset, basis in ((0x364, (1, 0, 0)), (0x370, (0, 1, 0)),
                          (0x37c, (0, 0, 1)), (0x490, (0, 0, -1)), (0x4a8, (1, 0, 0))):
        put(CAR + offset, *basis)
    put(CAR + 0xd58, 11., 12., fixture['old_forward'])
    put(CAR + 0x38c, .375)
    put(CAR + 0x434, 1234.)
    for offset in WHEELS:
        put(CAR + offset, 40., 50., 60.)
        put(CAR + offset + 0x78, 9.)
    put(CAR + 0xdb8, 7.)
    put(CAR + 0xdc0, 8.)
    uc.mem_write(CAR + 0x52c, bytes([fixture['alternate'] * 4]))
    uc.mem_write(0x628c88, struct.pack('<I', fixture['surface']))
    put(NORMAL, *normal)
    put(STACK + 0xc, *correction)
    uc.mem_write(STACK + 0x54, struct.pack('<I', EXIT))
    uc.mem_write(STACK + 0x68, struct.pack('<I', fixture['angular']))
    # At 0x4940ae, four arguments to the preceding scale call remain on stack.
    uc.reg_write(UC_X86_REG_ESP, STACK - 16)
    uc.reg_write(UC_X86_REG_EBX, CAR)
    uc.reg_write(UC_X86_REG_EDI, NORMAL)
    uc.reg_write(UC_X86_REG_FPCW, FPCW)
    uc.emu_start(0x4940ae, EXIT + 6, count=10000)
    if uc.reg_read(UC_X86_REG_EIP) != EXIT + 6:
        raise RuntimeError('instruction budget exhausted')

    actual = {'velocity': get(CAR + 0x33c, 3), 'position': get(CAR + 0x330, 3),
              'local_velocity': get(CAR + 0xd58, 3), 'speed': get(CAR + 0x35c),
              'return': get(RESULT), 'wheels': [x for off in WHEELS for x in get(CAR + off, 3)],
              'angular': get(CAR + 0x38c), 'contact_field_434': get(CAR + 0x434),
              'resets': [get(CAR + off + 0x78)[0] for off in WHEELS] + get(CAR + 0xdb8) + get(CAR + 0xdc0)}
    strength = f32(-sum(a * b for a, b in zip(velocity, normal)))
    expected = {'velocity': velocity, 'position': position, 'local_velocity': [11., 12., fixture['old_forward']],
                'angular': [.375], 'contact_field_434': [1234.],
                'speed': [speed], 'return': [0.], 'wheels': [40., 50., 60.] * 4,
                'resets': [9.] * 4 + [7., 8.]}
    if strength > 0:
        scale = f32(strength * f32(1.1))
        v = [f32(f32(a + f32(n * scale)) * f32(.998)) for a, n in zip(velocity, normal)]
        reported = f32(speed * f32(1 / 12)) if speed * .125 > strength else strength
        expected.update(velocity=v, position=[f32(a + b) for a, b in zip(position, correction)],
                        local_velocity=[-v[2], v[1], v[0]] if fixture['alternate'] else v,
                        speed=[approximate_speed(v)], return_value=[abs(f32(reported * 3))],
                        wheels=[f32(40 + correction[0]), 50., f32(60 + correction[2])] * 4)
        expected['return'] = expected.pop('return_value')
        if fixture['alternate']:
            expected['resets'] = [0.] * 6
        if fixture['angular']:
            expected['contact_field_434'] = [0.]
            expected['angular'] = [angular_response(strength, fixture['old_forward'], normal, .375)]
    for key, values in expected.items():
        if len(values) != len(actual[key]):
            raise AssertionError(f"{fixture['name']} {key}: length mismatch")
        for want, got in zip(values, actual[key]):
            if not math.isclose(want, got, rel_tol=2e-6, abs_tol=2e-6):
                raise AssertionError(f"{fixture['name']} {key}: expected {want}, original {got}")
    return {'input': fixture, 'original': actual}


def main():
    global FPCW
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--fixture-tsv', type=Path, help='Export original outputs for the Rust regression')
    parser.add_argument('--module', default='Porsche.exe', choices=['Porsche.exe', 'nfs5.exe'])
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--fpcw', type=lambda v: int(v, 0), default=FPCW, help='x87 control word (default 0x27f)')
    args = parser.parse_args()
    FPCW = args.fpcw
    modules = [json.loads(line) for line in (ROOT / 'research/binary-index/static/binaries.jsonl').read_text(encoding='utf-8').splitlines()]
    module, = [m for m in modules if m['file'] == args.module]
    data = (args.game_dir / module['path']).read_bytes()
    if hashlib.sha256(data).hexdigest() != module['sha256']:
        raise RuntimeError('original SHA256 mismatch')
    rng = random.Random(494000)
    vectors = [('head_on', [-10., 0., 0.], [1., 0., 0.]),
               ('departing', [10., 1., 4.], [1., 0., 0.]),
               ('stationary', [0., 0., 0.], [1., 0., 0.]),
               ('tangent', [0., 2., 40.], [1., 0., 0.]),
               ('glancing', [-.1, 2., 40.], [1., 0., 0.])]
    for index in range(27):
        angle = rng.uniform(-math.pi, math.pi)
        vectors.append((f'oblique_{index}', [rng.uniform(-80, 80) for _ in range(3)],
                        [math.cos(angle), 0., math.sin(angle)]))
    results = []
    for strength in (14.999, 15., 15.001):
        vectors.append((f'gate_{strength}', [-strength, 0., 0.], [1., 0., 0.]))
    for a in (-.6, .6):
        for z in (-.8, .8):
            vectors.append((f'quadrant_{a}_{z}', [-10*a, 0., -10*z], [a, 0., z]))
    for name, velocity, normal in vectors:
        for alternate in (False, True):
            for angular in (False, True):
                for old_forward in (19.999, 20., 20.001):
                    fixture = {'name': f'{name}_alt{int(alternate)}_angular{int(angular)}_forward{old_forward}',
                               'velocity': list(map(f32, velocity)), 'normal': list(map(f32, normal)),
                               'correction': list(map(f32, [.125, -.25, .5])),
                               'alternate': alternate, 'angular': angular, 'old_forward': f32(old_forward),
                               'surface': rng.choice([0, 0x10, 0x20, 0x60, 0xff])}
                    results.append(run_case(module, data, fixture))
    output = {'module': args.module, 'sha256': module['sha256'], 'entry': '0x4940ae',
              'scope': 'prepared geometry; optional angular callback enabled/disabled; original x86 scalar path in Unicorn 2.1.3',
              'cases_passed': len(results), 'results': results}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + '\n', encoding='utf-8')
    if args.fixture_tsv:
        header = ['name', 'vx', 'vy', 'vz', 'nx', 'ny', 'nz', 'alternate', 'angular', 'old_forward']
        fields = [('velocity', 3), ('position', 3), ('local_velocity', 3), ('speed', 1),
                  ('return', 1), ('wheels', 12), ('resets', 6), ('angular', 1), ('contact_field_434', 1)]
        header += [f'out_{name}_{i}' for name, count in fields for i in range(count)]
        rows = ['\t'.join(header)]
        for result in results:
            fixture = result['input']
            row = [fixture['name'], *fixture['velocity'], *fixture['normal'],
                   int(fixture['alternate']), int(fixture['angular']), fixture['old_forward']]
            row += [v for name, _ in fields for v in result['original'][name]]
            rows.append('\t'.join(map(str, row)))
        args.fixture_tsv.parent.mkdir(parents=True, exist_ok=True)
        args.fixture_tsv.write_text('\n'.join(rows) + '\n', encoding='utf-8')
    print(f'{len(results)} original-x86 differential cases passed; {args.output}')


if __name__ == '__main__':
    main()
