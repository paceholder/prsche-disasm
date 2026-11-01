"""Record the original parts code of nfs5.exe (prices, parts market, spare parts, repair).

Runs unchanged in Unicorn (career_harness.CareerMachine):

- prices of every part at several years and conditions: 0x5181c0 (inflated price),
  Part_Value 0x518220, Part_RepairCost 0x518880, 0x518900 (replace cost);
- Parts_BuildMarket 0x517d10 for owned cars (built by the original 0x4ee3b0) and years;
- on the spare parts `[0x69a0ec]`: buy 0x5183c0, sell 0x5182c0, install 0x518510, uninstall
  0x518740, the fitting spares 0x517f70, the listed spares 0x518b80; on installed parts: fix
  0x5189f0, replace 0x518a80 (cash through the Player_AddCash 0x5012e0 callback);
- the repair screen's group costs 0x4efbc0 / 0x4efc20 / 0x4efc80 / 0x4efce0 / 0x4efd40 /
  0x4efda0 / 0x4efe60 / 0x4efec0 / 0x4eff20 / 0x4eff80 and repairs 0x4f0490 / 0x4f04e0 /
  0x4f0530 / 0x4f0580 / 0x4f05d0 / 0x4f0620 / 0x4f06b0 / 0x4f0700 / 0x4f0750 / 0x4f0830 at the
  career year [0x5db764].

Wear is set by the recorder (`wear` op writes +0x164 of an installed copy, `zones` the ten
words +0x6a8), as the race would.

    PYTHON scripts/research/replay-career-parts.py --game-dir local/game --data synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-game/tests/fixtures/career-parts-synthetic.tsv
    PYTHON scripts/research/replay-career-parts.py --game-dir local/game --data local/game \
        --out local/fixtures/career-parts-car.tsv

Format: `car`, `part` table lines as replay-career-garage.py, then op rows: step, op, args,
ret, cash, FNV-1a 64 of the spares string (`id:condition` joined by '/', '-' if empty) and of
the op's car string (installed `id:condition` list, a space, the ten zone words; '-' for none).
Conditions are Python reprs of the f32 widened to double.
"""
import argparse
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from career_harness import (CAR_RECORD, OWNED_CAR, PART_RECORD, CareerMachine, fnv64,  # noqa: E402
                            records, synthetic_catalogue, synthetic_parts)
from vehicle_harness import load_module  # noqa: E402

G = {'year': 0x5db764, 'cash': 0x691d90, 'parts_n': 0x628f80, 'parts': 0x628f84}
SPARES, SPARE_LIST, REGISTRY = 0x69a0ec, 0x69a0e8, 0x69188c
CONSTRUCT, ADD_CASH = 0x4ee3b0, 0x5012e0
INFLATED, VALUE, REPAIR_COST, REPLACE_COST = 0x5181c0, 0x518220, 0x518880, 0x518900
MARKET, FITTING, LISTED = 0x517d10, 0x517f70, 0x518b80
BUY, SELL, INSTALL, UNINSTALL, FIX, REPLACE = 0x5183c0, 0x5182c0, 0x518510, 0x518740, 0x5189f0, 0x518a80
COSTS = [0x4efbc0, 0x4efc20, 0x4efc80, 0x4efce0, 0x4efd40, 0x4efda0, 0x4efe60, 0x4efec0, 0x4eff20, 0x4eff80]
REPAIRS = [0x4f0490, 0x4f04e0, 0x4f0530, 0x4f0580, 0x4f05d0, 0x4f0620, 0x4f06b0, 0x4f0700, 0x4f0750, 0x4f0830]


def s32(v):
    return v - (1 << 32) if v & 0x80000000 else v


class Parts:
    def __init__(self, m, cars, parts):
        self.m = m
        self.cars, self.parts = cars, parts
        m.put_u32(G['parts_n'], len(parts))
        self.part_base = m.data_block(b''.join(parts))
        self.car_base = m.data_block(b''.join(cars))
        m.global_vector(SPARES, [])
        m.global_vector(SPARE_LIST, [])
        m.global_vector(REGISTRY, [])
        m.put_u32(G['parts'], self.part_base)
        m.put_u32(G['cash'], 5_000_000)
        m.put_u32(G['year'], 1950)
        self.owned = []          # car object addresses
        self.rows = []
        self.step = 0
        self.scratch = m.alloc(PART_RECORD)

    def part_index(self, p):
        return (p - self.part_base) // PART_RECORD

    def items(self, va):
        return self.m.vector_items(self.m.get_u32(va))

    def part_str(self, p):
        return f'{self.m.i16(p)}:{self.m.get(p + 0x164)[0]!r}'

    def spares(self):
        return '/'.join(self.part_str(p) for p in self.items(SPARES)) or '-'

    def car_str(self, k):
        if k is None:
            return '-'
        car = self.owned[k]
        inst = self.m.vector_items(self.m.get_u32(car + 0x758))
        zones = ','.join(repr(v) for v in self.m.get(car + 0x6a8, 10))
        return ('/'.join(self.part_str(p) for p in inst) or '-') + ' ' + zones

    def record(self, op, args, ret, car=None):
        self.rows.append([self.step, op, ','.join(str(a) for a in args) or '-', ret,
                          self.m.get_i32(G['cash'])[0], f'{fnv64(self.spares().encode()):016x}',
                          f'{fnv64(self.car_str(car).encode()):016x}'])
        self.step += 1

    def year(self, y):
        self.m.put_u32(G['year'], y)

    # ops ----------------------------------------------------------------------
    def new_car(self, k):
        m = self.m
        obj = m.alloc(OWNED_CAR)
        m.call(CONSTRUCT, self.car_base + CAR_RECORD * k, 1, 2, 3, 4, 5, 6, regs={'ecx': obj},
               budget=20_000_000)
        reg = m.get_u32(REGISTRY)
        self.owned.append(obj)
        self.record('newcar', [k], len(m.vector_items(reg)), car=len(self.owned) - 1)

    def price(self, k, year, cond):
        m = self.m
        m.put_bytes(self.scratch, self.parts[k])
        m.put(self.scratch + 0x164, cond)
        r = [s32(m.call(va, self.scratch, year)) for va in (INFLATED, VALUE, REPAIR_COST, REPLACE_COST)]
        self.record('price', [k, year, repr(m.get(self.scratch + 0x164)[0])], ':'.join(map(str, r)))

    def market(self, car, year):
        m = self.m
        out = m.vector([])
        m.call(MARKET, out, self.owned[car], year, budget=20_000_000)
        self.record('market', [car, year], '/'.join(str(self.part_index(p)) for p in m.vector_items(out)) or '-')

    def fitting(self, car):
        m = self.m
        out = m.vector([])
        m.call(FITTING, out, self.owned[car], budget=20_000_000)
        spares = self.items(SPARES)
        self.record('fitting', [car], '/'.join(str(spares.index(p)) for p in m.vector_items(out)) or '-')

    def listed(self):
        m = self.m
        m.call(LISTED, budget=20_000_000)
        spares = self.items(SPARES)
        self.record('listed', [], '/'.join(str(spares.index(p)) for p in self.items(SPARE_LIST)) or '-')

    def buy(self, k, year):
        self.m.call(BUY, self.part_base + PART_RECORD * k, year, ADD_CASH, budget=20_000_000)
        self.record('buy', [k, year], 0)

    def sell(self, i, year):
        self.m.call(SELL, self.items(SPARES)[i], year, ADD_CASH, budget=20_000_000)
        self.record('sell', [i, year], 0)

    def install(self, car, i):
        self.m.call(INSTALL, self.items(SPARES)[i], self.owned[car], budget=20_000_000)
        self.record('install', [car, i], 0, car=car)

    def installed(self, car):
        return self.m.vector_items(self.m.get_u32(self.owned[car] + 0x758))

    def uninstall(self, car, i):
        self.m.call(UNINSTALL, self.installed(car)[i], self.owned[car], budget=20_000_000)
        self.record('uninstall', [car, i], 0, car=car)

    def wear(self, car, i, cond):
        self.m.put(self.installed(car)[i] + 0x164, cond)
        self.record('wear', [car, i, repr(self.m.get(self.installed(car)[i] + 0x164)[0])], 0, car=car)

    def zones(self, car, vals):
        self.m.put(self.owned[car] + 0x6a8, *vals)
        self.record('zones', [car] + [repr(v) for v in self.m.get(self.owned[car] + 0x6a8, 10)], 0, car=car)

    def fix(self, car, i, year, replace=False):
        self.m.call(REPLACE if replace else FIX, self.installed(car)[i], year, ADD_CASH, budget=20_000_000)
        self.record('replace' if replace else 'fix', [car, i, year], 0, car=car)

    def cost(self, car, g, year):
        self.year(year)
        r = s32(self.m.call(COSTS[g], regs={'ecx': self.owned[car]}, budget=20_000_000))
        self.record('cost', [car, g, year], r, car=car)

    def repair(self, car, g, year):
        self.year(year)
        self.m.call(REPAIRS[g], regs={'ecx': self.owned[car]}, budget=20_000_000)
        self.record('repair', [car, g, year], 0, car=car)


def has_body(p, parts):
    return any(struct.unpack_from('<h', parts[p.part_index(q)], 0x26)[0] == 10 and
               parts[p.part_index(q)][0x28:0x2d] == b'Body\0' for q in p.installed(0))


def play(p, seed):
    rng = random.Random(seed)
    parts = p.parts
    for k in range(len(parts)):
        year = struct.unpack_from('<h', parts[k], 0x46)[0]
        for y in sorted({year - 1, year, year + 1, year + 10, year + 61, year + 70}):
            p.price(k, y, rng.choice((0.0, 0.0, 10.0, 37.5, 49.9, 50.0, 64.0, -3.0)))
    p.price(0, 2000, 12.345678)
    # Cars whose stock includes a body part when possible (the body repair needs one).
    order = list(range(len(p.cars)))
    rng.shuffle(order)
    for k in order[:8]:
        p.new_car(k)
    for car in range(len(p.owned)):
        for y in (1950, 1970, 1989, 2000):
            p.market(car, y)
    for _ in range(300):
        r = rng.random()
        car = rng.randrange(len(p.owned))
        spares = p.items(SPARES)
        inst = p.installed(car)
        year = rng.choice((1950, 1960, 1975, 1990, 2000))
        p.year(year)
        if r < 0.2:
            p.buy(rng.randrange(len(parts)), year)
        elif r < 0.27 and spares:
            p.sell(rng.randrange(len(spares)), year)
        elif r < 0.4 and spares:
            p.install(car, rng.randrange(len(spares)))
        elif r < 0.45 and inst:
            p.uninstall(car, rng.randrange(len(inst)))
        elif r < 0.6 and inst:
            p.wear(car, rng.randrange(len(inst)), rng.choice((5.0, 12.5, 33.3, 50.0, 75.0)))
        elif r < 0.63:
            p.zones(car, [rng.choice((0.0, 0.5, 3.0)) for _ in range(10)])
        elif r < 0.72 and inst:
            p.fix(car, rng.randrange(len(inst)), year, replace=rng.random() < 0.5)
        elif r < 0.8:
            p.cost(car, rng.randrange(10), year)
        elif r < 0.88:
            g = rng.randrange(10)
            if g in (8, 9) and not any(p.m.i16(q + 0x26) == 10 and p.m.cstr(q + 0x28, 8) == 'Body'
                                       for q in inst):
                g = 0             # the original fixes NULL when the car has no "Body" part
            p.repair(car, g, year)
        elif r < 0.94:
            p.fitting(car)
        else:
            p.listed()


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--data', required=True, help="'synthetic' or a game dir with FEData/Data")
    ap.add_argument('--seed', type=int, default=5)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    module, data = load_module('nfs5.exe', a.game_dir)
    m = CareerMachine(module, data)
    if a.data == 'synthetic':
        parts = synthetic_parts()
        cars = synthetic_catalogue(parts)
    else:
        d = Path(a.data) / 'FEData/Data'
        parts = records((d / 'nfs5.prt').read_bytes(), PART_RECORD)
        cars = records((d / 'nfs5.car').read_bytes(), CAR_RECORD)
    p = Parts(m, cars, parts)
    play(p, a.seed)
    lines = ['# replay-career-parts.py: part prices, Parts_BuildMarket 0x517d10, spare parts, install,',
             '# fix/replace and the repair screen functions of nfs5.exe run in Unicorn.',
             f'# data: {"synthetic (no game data)" if a.data == "synthetic" else "nfs5.car + nfs5.prt of the user"}']
    lines += [f'car\t{k}\t{c.hex()}' for k, c in enumerate(cars)]
    lines += [f'part\t{k}\t{q.hex()}' for k, q in enumerate(parts)]
    lines.append('\t'.join(['step', 'op', 'args', 'ret', 'cash', 'spares', 'car']))
    lines += ['\t'.join(str(v) for v in r) for r in p.rows]
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{len(p.rows)} ops -> {a.out}')


if __name__ == '__main__':
    main()
