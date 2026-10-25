"""Record original track-object collision outputs by executing nfs5.exe x86 code.

Runs Car_ObjectNearTest (0x498280), the cylinder response (0x498aa0, with the
block response 0x498380 and its callees) and the box response (0x498690, with
the vtable segment test 0x486d70 and the body-point contact 0x496610/0x495f30)
in Unicorn on synthetic cars and objects, and writes TSV fixtures. The inputs
are synthetic only, so the fixtures are committed with the engine tests. No
original instruction is patched. Field meanings: docs/re/track-objects.md in
the workspace root.

Each row stores the car struct before and after as sparse lists of changed
dwords `offset:hex`, so the Rust test can check that every dword the original
wrote is modelled.

    python3 scripts/research/replay-track-objects.py --game-dir <game/app> \
        --out-dir <crate>/tests/fixtures
"""
import argparse
import importlib.util
import math
from pathlib import Path
import random
import struct

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('pair', HERE / 'replay-car-pair.py')
pair = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pair)

CAR, OBJ, RES = pair.CAR_A, 0x2300000, pair.RES
CAR_DUMP = 0x1200
NEAR, CYL, BOX = 0x498280, 0x498aa0, 0x498690
DYN = 0x4a1030   # Car_ObjectDynamicsTick (flag-0x80 object cars)
REAL_CAR = pair.CAR_B
G_DAMAGE_ON, G_RAND, G_MODE, G_PAIR_DAMP = pair.G_DAMAGE_ON, pair.G_RAND, pair.G_MODE, pair.G_PAIR_DAMP
VT_CYL, VT_BOX = 0x5b490c, 0x5b4938
f32 = pair.f32


def dump(m, base, size):
    raw = bytes(m.uc.mem_read(base, size))
    return struct.unpack('<%dI' % (size // 4), raw)


def sparse(words, ref=None):
    return ';'.join(f'{i * 4:x}:{w:08x}' for i, w in enumerate(words)
                    if (w != 0 if ref is None else w != ref[i]))


def make_car(pos, yaw, vel, ang=(0., 0., 0.), ext=(0.9, 0.65, 2.1), mass=1.25, speed=None, flags=0):
    c = pair.make_car(pos, yaw, vel, ang, ext, mass)
    if speed is not None:
        c['speed'] = speed
    c['flags'] = flags
    return c


def setup_car(m, car, damage, mode=0):
    pair.write_car(m, CAR, car)
    m.uc.mem_write(pair.RES, bytes(0x1000))
    ax_r, ax_u, ax_f = car['axis_r'], car['axis_u'], car['axis_f']
    hw, hh, hl = car['ext']
    pts = []
    for sr, sf in ((-1, 1), (1, 1), (-1, -1), (1, -1)):   # FL, FR, RL, RR
        pts.append([car['pos'][i] + sr * hw * ax_r[i] - hh * ax_u[i] + sf * hl * ax_f[i] for i in range(3)])
    m.put_u32(CAR + 0x32c, 4)
    for i, p in enumerate(pts):
        m.put(CAR + 0x1dc + 12 * i, *map(f32, p))
        m.put(CAR + 0x7f8 + 0xc4 * i, *map(f32, p))
    # ground frame, local velocity and misc fields the response reads
    m.put(CAR + 0x490, *ax_r)
    m.put(CAR + 0x4a8, *ax_f)
    v = car['vel']
    m.put(CAR + 0xd58, *map(f32, [sum(v[i] * ax_r[i] for i in range(3)), sum(v[i] * ax_u[i] for i in range(3)),
                                  sum(v[i] * ax_f[i] for i in range(3))]))
    m.put(CAR + 0x3a0, 0.5)
    m.put(CAR + 0x424, 0.05)
    m.put(CAR + 0x428, 0.0)
    m.uc.mem_write(CAR + 0x52c, struct.pack('<I', car['flags']))
    m.put_u32(CAR + 0x558, pair.RES)
    m.put_u32(G_PAIR_DAMP, 0)
    m.put_u32(G_DAMAGE_ON, int(damage))
    m.put_u32(G_MODE, mode)
    m.put_u32(G_RAND, 0x1234)


def cyl_obj(m, centre, radius=0.25, id_=0x1234):
    m.uc.mem_write(OBJ, bytes(0x100))
    m.put_u32(OBJ, VT_CYL)
    m.put(OBJ + 8, centre[0] - 0.5, centre[1] - 1., centre[2] - 0.5, centre[0] + 0.5, centre[1] + 1., centre[2] + 0.5)
    m.put_u32(OBJ + 0x20, id_)
    m.put(OBJ + 0x34, radius)


def box_obj(m, corners, id_=0x4321):
    """corners: 4 points (x, 0, z) of the footprint rectangle in order."""
    m.uc.mem_write(OBJ, bytes(0x100))
    m.put_u32(OBJ, VT_BOX)
    xs, zs = [c[0] for c in corners], [c[2] for c in corners]
    m.put(OBJ + 8, min(xs), -1., min(zs), max(xs), 3., max(zs))
    m.put_u32(OBJ + 0x20, id_)
    for i, c in enumerate(corners):
        m.put(OBJ + 0x34 + 12 * i, f32(c[0]), 0., f32(c[2]))
    cx, cz = (min(xs) + max(xs)) / 2, (min(zs) + max(zs)) / 2
    m.put(OBJ + 0x64, f32(1.05 * math.hypot(max(xs) - cx, max(zs) - cz)))


def rect(cx, cz, hx, hz, yaw):
    c, s = math.cos(yaw), math.sin(yaw)
    pts = []
    for sx, sz in ((-1, -1), (-1, 1), (1, 1), (1, -1)):
        x, z = sx * hx, sz * hz
        pts.append([f32(cx + c * x + s * z), 0., f32(cz - s * x + c * z)])
    return pts


def run(m, kind, name, car, obj_setup, damage, extra=None):
    setup_car(m, car, damage)
    obj_setup()
    before_c, before_o = dump(m, CAR, CAR_DUMP), dump(m, OBJ, 0x100)
    arg = 0
    ret = m.call({'cyl': CYL, 'box': BOX}[kind], CAR, OBJ)
    after_c = dump(m, CAR, CAR_DUMP)
    return {'kind': kind, 'name': name, 'damage': damage, 'car_in': sparse(before_c), 'obj_in': sparse(before_o),
            'car_chg': sparse(after_c, before_c), 'ret': ret}


def dyn_case(m, name, car, dormant, active, real_pos, real_active):
    """Car_ObjectDynamicsTick: object car state and one real car (list entry 0)."""
    setup_car(m, car, 0)
    m.uc.mem_write(CAR + 0x7d, bytes([1 if dormant else 0]))
    m.uc.mem_write(CAR + 0x7c, bytes([1 if active else 0]))
    m.put(CAR + 0x424, car['h424'])
    m.put(CAR + 0x428, car['h428'])
    m.uc.mem_write(REAL_CAR, bytes(0x1000))
    m.put(REAL_CAR + 0x330, *real_pos)
    m.uc.mem_write(REAL_CAR + 0x7f, bytes([1 if real_active else 0]))
    m.put_u32(0x5e8b58, 1)
    m.put_u32(0x5e8b34, REAL_CAR)
    before = dump(m, CAR, CAR_DUMP)
    m.call(DYN, CAR)
    after = dump(m, CAR, CAR_DUMP)
    return {'kind': 'dyn', 'name': name, 'damage': 0, 'car_in': sparse(before),
            'obj_in': sparse(dump(m, REAL_CAR, 0x400)), 'car_chg': sparse(after, before), 'ret': 0}


def near_case(m, name, car, centre, radius):
    setup_car(m, car, 0)
    m.put(0x2310000, *map(f32, centre))
    ret = m.call(NEAR, CAR, 0x2310000, struct.unpack('<I', struct.pack('<f', f32(radius)))[0])
    return {'kind': 'near', 'name': name, 'damage': 0, 'car_in': sparse(dump(m, CAR, CAR_DUMP)),
            'obj_in': f'0:{struct.unpack("<I", struct.pack("<f", f32(centre[0])))[0]:08x};4:{struct.unpack("<I", struct.pack("<f", f32(centre[1])))[0]:08x};8:{struct.unpack("<I", struct.pack("<f", f32(centre[2])))[0]:08x};c:{struct.unpack("<I", struct.pack("<f", f32(radius)))[0]:08x}',
            'car_chg': '', 'ret': ret & 0xff}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--module', default='nfs5.exe')
    ap.add_argument('--game-dir', type=Path, required=True)
    ap.add_argument('--out-dir', type=Path, required=True)
    ap.add_argument('--random', type=int, default=150)
    args = ap.parse_args()
    module, data = pair.load_module(args.module, args.game_dir)
    m = pair.Machine(module, data)
    m.uc.mem_map(OBJ, 0x20000)
    rng = random.Random(4986)
    rows = []

    # cylinder cases: car at pos heading towards cylinder at origin
    def cyl_cases():
        yield 'cyl_head_on', make_car([0., .65, 2.4], math.pi, [0., 0., -25.], speed=25.), (0., 0., 0.)
        yield 'cyl_head_on_slow', make_car([0., .65, 2.4], math.pi, [0., 0., -3.], speed=3.), (0., 0., 0.)
        yield 'cyl_offset', make_car([0.7, .65, 2.3], math.pi, [0., 0., -20.], speed=20.), (0., 0., 0.)
        yield 'cyl_side', make_car([1.2, .65, 0.3], 0., [0., 0., -15.], speed=15.), (0., 0., 0.)
        yield 'cyl_deep_flip', make_car([0., .65, 0.5], math.pi, [0., 0., -20.], speed=20.), (0., 0., 0.)
        yield 'cyl_far', make_car([0., .65, 6.], math.pi, [0., 0., -20.], speed=20.), (0., 0., 0.)
        yield 'cyl_centre', make_car([0., .65, 0.], math.pi, [0., 0., -20.], speed=20.), (0., 0., 0.)
        yield 'cyl_flags80', make_car([0., .65, 2.4], math.pi, [0., 0., -25.], speed=25., flags=0x80), (0., 0., 0.)
        for i in range(args.random):
            yaw = rng.uniform(-math.pi, math.pi)
            ang = rng.uniform(-math.pi, math.pi)
            d = rng.uniform(0.3, 3.2)
            v = rng.uniform(0, 40)
            hd = rng.uniform(-math.pi, math.pi)
            vel = [v * math.sin(hd), rng.uniform(-1, 1), v * math.cos(hd)]
            yield f'cyl_rand_{i}', make_car([d * math.cos(ang), .65, d * math.sin(ang)], yaw, vel,
                                            ang=(0., rng.uniform(-3, 3), 0.), mass=rng.uniform(.9, 1.6),
                                            speed=max(abs(vel[0]), abs(vel[2])) + .25 * min(abs(vel[0]), abs(vel[2]))), \
                (0., 0., 0.)

    for name, car, c in cyl_cases():
        for damage in (0, 1):
            rows.append(run(m, 'cyl', name, car, lambda c=c: cyl_obj(m, c), damage))

    def box_cases():
        base = rect(0., 0., 1.0, 0.6, 0.)
        yield 'box_head_on', make_car([0., .65, 4.3], math.pi, [0., 0., -25.], speed=25.), base
        yield 'box_head_on_slow', make_car([0., .65, 3.0], math.pi, [0., 0., -4.], speed=4.), base
        yield 'box_corner', make_car([1.9, .65, 3.2], 0.3, [-3., 0., -20.], speed=20.), base
        yield 'box_side', make_car([2.4, .65, 0.], 0., [-12., 0., 0.], speed=12.), base
        yield 'box_inside', make_car([0., .65, 0.], 0., [0., 0., -10.], speed=10.), base
        yield 'box_none', make_car([6., .65, 6.], 0., [0., 0., -10.], speed=10.), base
        yield 'box_rotated', make_car([0., .65, 3.5], math.pi, [0., 0., -22.], speed=22.), rect(0., 0., 1.0, 0.6, 0.6)
        for i in range(args.random):
            yaw = rng.uniform(-math.pi, math.pi)
            ang = rng.uniform(-math.pi, math.pi)
            d = rng.uniform(0.5, 4.5)
            v = rng.uniform(0, 40)
            hd = rng.uniform(-math.pi, math.pi)
            vel = [v * math.sin(hd), rng.uniform(-1, 1), v * math.cos(hd)]
            hx, hz = rng.uniform(.4, 1.5), rng.uniform(.4, 1.5)
            yield f'box_rand_{i}', make_car([d * math.cos(ang), .65, d * math.sin(ang)], yaw, vel,
                                            ang=(0., rng.uniform(-3, 3), 0.), mass=rng.uniform(.9, 1.6),
                                            speed=max(abs(vel[0]), abs(vel[2])) + .25 * min(abs(vel[0]), abs(vel[2]))), \
                rect(0., 0., hx, hz, rng.uniform(-math.pi, math.pi))

    for name, car, corners in box_cases():
        for damage in (0, 1):
            rows.append(run(m, 'box', name, car, lambda c=corners: box_obj(m, c), damage))

    # near test
    for i in range(args.random):
        yaw = rng.uniform(-math.pi, math.pi)
        car = make_car([0., .65, 0.], yaw, [0., 0., 0.])
        centre = (rng.uniform(-5, 5), rng.uniform(-1, 1), rng.uniform(-5, 5))
        rows.append(near_case(m, f'near_{i}', car, centre, rng.choice([0.25, 0.25, 1.0, 1.7])))

    # object dynamics tick
    for i in range(args.random):
        car = make_car([rng.uniform(-20, 20), .65, rng.uniform(-20, 20)], 0., [rng.uniform(-8, 8) * rng.choice([0, 1]),
                       rng.uniform(-3, 3) * rng.choice([0, 1]), rng.uniform(-8, 8) * rng.choice([0, 1])],
                       ang=tuple(rng.uniform(-6, 6) * rng.choice([0, 0, 1]) for _ in range(3)))
        car['h424'] = rng.choice([0., 0., 0.003, 0.2, 1.0])
        car['h428'] = rng.choice([0., 0.003, 0.5])
        real = (rng.uniform(-30, 30), .65, rng.uniform(-30, 30))
        rows.append(dyn_case(m, f'dyn_{i}', car, rng.random() < .4, rng.random() < .6, real, rng.random() < .8))
    header = ['kind', 'name', 'damage', 'ret', 'car_in', 'obj_in', 'car_chg']
    args.out_dir.mkdir(parents=True, exist_ok=True)
    lines = ['\t'.join(header)] + ['\t'.join(str(r[k]) for k in header) for r in rows]
    (args.out_dir / 'track-objects.tsv').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    kinds = {}
    for r in rows:
        key = (r['kind'], bool(r['car_chg']))
        kinds[key] = kinds.get(key, 0) + 1
    print(f'{len(rows)} original runs; (kind, changed): {kinds}')


if __name__ == '__main__':
    main()
