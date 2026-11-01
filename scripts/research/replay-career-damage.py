"""Record the original career race-damage write-back of nfs5.exe (Record_LoadZoneDamage 0x4edd40).

Runs 0x4edd40 unchanged in Unicorn (career_harness.CareerMachine) with its callees 0x4f0480
(installed-parts vector) and 0x4f0400 (the "Body" part). The cars are real owned-car objects
built by the original constructor 0x4ee3b0 (as replay-career-parts.py); the player's config
block at `0x5e75a0 + 600 * player` is filled by the recorder as the race would leave it:
zones `+0x1c4` (10 f32), part ids `+0x124` (20 i32), part wear `+0x174` (20 f32); the globals
`[0x690ad8]` player and `[0x690adc]` the car object are set. Cars without a "Body" part are
skipped (the original writes through NULL).

    PYTHON scripts/research/replay-career-damage.py --game-dir local/game \
        --out iterations/012-campaign-fidelity/crates/nfs-game/tests/fixtures/career-damage-synthetic.tsv

Format: `car` / `part` table lines and op rows as replay-career-parts.py; op `racedamage` has
args `car, player, z0..z9, id0..id19, wear0..wear19` (floats as Python reprs of the f32), its
`car` column hashes the car string (installed `id:condition` list, a space, the ten zone
words; f32 values as 8 hex digits of their bits). Synthetic data only (no game content).
"""
import argparse
import importlib.util
import random
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from career_harness import CareerMachine, synthetic_catalogue, synthetic_parts  # noqa: E402
from vehicle_harness import f32, load_module  # noqa: E402

spec = importlib.util.spec_from_file_location('replay_career_parts', HERE / 'replay-career-parts.py')
rcp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rcp)

LOAD_ZONE_DAMAGE = 0x4edd40
G_PLAYER, G_CAR = 0x690ad8, 0x690adc
BLOCK, BLOCK_STRIDE = 0x5e75a0, 600


class Damage(rcp.Parts):
    def car_str(self, k):
        # f32 bit patterns (Python's repr and Rust's Debug break exact decimal ties apart).
        car = self.owned[k]
        bits = lambda a: f'{self.m.get_u32(a):08x}'  # noqa: E731
        inst = '/'.join(f'{self.m.i16(q)}:{bits(q + 0x164)}'
                        for q in self.m.vector_items(self.m.get_u32(car + 0x758))) or '-'
        return inst + ' ' + ','.join(bits(car + 0x6a8 + 4 * z) for z in range(10))

    def body(self, car):
        return any(self.m.i16(q + 0x26) == 10 and self.m.cstr(q + 0x28, 8) == 'Body'
                   for q in self.installed(car))

    def race_damage(self, car, player, zones, ids, wear):
        m = self.m
        b = BLOCK + BLOCK_STRIDE * player
        m.put(b + 0x1c4, *zones)
        m.put_i32(b + 0x124, *ids)
        m.put(b + 0x174, *wear)
        m.put_u32(G_PLAYER, player)
        m.put_u32(G_CAR, self.owned[car])
        m.call(LOAD_ZONE_DAMAGE, budget=20_000_000)
        args = [car, player] + [repr(f32(v)) for v in zones] + ids + [repr(f32(v)) for v in wear]
        self.record('racedamage', args, 0, car=car)


def play(p, seed):
    rng = random.Random(seed)
    order = list(range(len(p.cars)))
    rng.shuffle(order)
    for k in order[:12]:
        p.new_car(k)
    cars = [c for c in range(len(p.owned)) if p.body(c)]
    if not cars:
        raise RuntimeError('no synthetic car with a Body part')
    for _ in range(200):
        car = rng.choice(cars)
        inst = p.installed(car)
        installed_ids = [p.m.i16(q) for q in inst]
        zones = [rng.choice((0.0, 0.0, 12.5, 30.0, 33.3, 50.0, 99.9, 100.0, rng.uniform(0, 100)))
                 for _ in range(10)]
        ids = [rng.choice(installed_ids) if installed_ids and rng.random() < 0.6
               else rng.randint(-5, 1200) for _ in range(20)]
        wear = [rng.choice((0.0, -0.0, -4.0, 5.0, 37.5, 100.0, rng.uniform(-10, 100))) for _ in range(20)]
        if rng.random() < 0.2 and inst:
            p.wear(car, rng.randrange(len(inst)), rng.choice((5.0, 50.0)))
        p.race_damage(car, rng.randrange(2), zones, ids, wear)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--seed', type=int, default=9)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    module, data = load_module('nfs5.exe', a.game_dir)
    m = CareerMachine(module, data)
    parts = synthetic_parts()
    cars = synthetic_catalogue(parts)
    p = Damage(m, cars, parts)
    play(p, a.seed)
    lines = ['# replay-career-damage.py: Record_LoadZoneDamage 0x4edd40 of nfs5.exe run in Unicorn',
             '# on owned cars built by 0x4ee3b0. data: synthetic (no game data)']
    lines += [f'car\t{k}\t{c.hex()}' for k, c in enumerate(cars)]
    lines += [f'part\t{k}\t{q.hex()}' for k, q in enumerate(parts)]
    lines.append('\t'.join(['step', 'op', 'args', 'ret', 'cash', 'spares', 'car']))
    lines += ['\t'.join(str(v) for v in r) for r in p.rows]
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{len(p.rows)} ops -> {a.out}')


if __name__ == '__main__':
    main()
