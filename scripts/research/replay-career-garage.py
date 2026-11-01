"""Record the original dealer and garage code of nfs5.exe (Evolution) in Unicorn.

Runs unchanged (career_harness.CareerMachine: bump heap, fake FE accessors):

- Car_DealerPrice 0x4ee220 for every catalogue car and year 1948..2002;
- Career_RebuildCarLists 0x4f3e40 with game type 4 (the `Game.GameType` accessor returns 4)
  and the all-cars cheat byte [0x691a04] off/on: the lists 0x6919ec (catalogue cars),
  0x6919f0 (model file present), 0x6919f4 (model file present and first year <= year),
  0x6919fc (+0x1f4 == 1), 0x6919f8 (+0x1f4 == 2), the dealer list 0x691a00, each sorted by
  0x4f0f50 (0x4f1d40: era +0x1b0, class +0x1ac, first year +0x1f8), and the owned cars
  usable this year 0x691dfc (0x501ad0). The model-file check 0x59dc30 is stubbed with a set of
  model names "present" (sprintf 0x5a0fbf stubbed to write the model name);
  the track list builder 0x4fbc10 is stubbed;
- Garage_BuyDealerCar 0x501cf0 with the `DealerCar.Car` accessor returning a dealer-list car
  and given colour words: cash check (string 802), 80-car limit (string 61), the owned car
  object 0x4ee3b0 / 0x4eeba0 / 0x4ee9a0 (catalogue copy, colours, tuning defaults, stock parts
  from the parts table [0x628f84] count [0x628f80], installed copies sorted by 0x49c350),
  price paid +0x6a4, owned list sort 0x4f0ee0, cash change;
- Garage_AddPrizeCar 0x502270 (owned car with price paid 0, list not sorted);
- Player_AddCash 0x5012e0.

Owned cars are dumped as the FNV-1a 64 of their 0x760-byte image (vector pointers +0x758 and
+0x75c zeroed) plus the installed (+0x758) and stock (+0x75c) part ids and the FNV of the
installed part copies.

Tables: --data synthetic (career_harness generators; committed fixture) or --data <game dir>
(the user's FEData/Data/nfs5.car + nfs5.prt; local fixture).

    PYTHON scripts/research/replay-career-garage.py --game-dir local/game --data synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-game/tests/fixtures/career-garage-synthetic.tsv
    PYTHON scripts/research/replay-career-garage.py --game-dir local/game --data local/game \
        --out local/fixtures/career-garage-car.tsv

Format: `car <index> <hex>` and `part <index> <hex>` lines (the tables), `present <model>`,
then op rows: step, op, args, ret, cash, lists, garage (count:FNV of the per-car dump; the
full dump on the last row `final`), calls.
"""
import argparse
import random
import struct
import sys

from unicorn.x86_const import UC_X86_REG_ECX
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from career_harness import (CAR_RECORD, OWNED_CAR, PART_RECORD, CareerMachine, fnv64,  # noqa: E402
                            records, synthetic_catalogue, synthetic_parts)
from vehicle_harness import load_module  # noqa: E402

G = {'year': 0x5db764, 'cash': 0x691d90, 'cheat': 0x691a04, 'parts_n': 0x628f80, 'parts': 0x628f84}
LISTS = {'all': 0x6919ec, 'model': 0x6919f0, 'year': 0x6919f4, 'f1': 0x6919fc, 'f2': 0x6919f8,
         'dealer': 0x691a00}
CATALOGUE, OWNED, OWNED_AVAIL, REGISTRY, USED = 0x6919e8, 0x691df8, 0x691dfc, 0x69188c, 0x691b88
PRICE, REBUILD, BUY, PRIZE, ADD_CASH = 0x4ee220, 0x4f3e40, 0x501cf0, 0x502270, 0x5012e0
S_GAMETYPE, S_DEALER, S_USED = 0x5cd698, 0x5da500, 0x5da6c4


class Garage:
    def __init__(self, m, cars, parts, present):
        self.m = m
        self.cars, self.parts, self.present = cars, parts, present
        self.calls = []
        self.dialogs = []
        m.put_u32(G['parts_n'], len(parts))
        m.put_u32(G['parts'], m.data_block(b''.join(parts)))
        self.car_base = m.data_block(b''.join(cars))
        self.car_ptrs = [self.car_base + CAR_RECORD * k for k in range(len(cars))]
        m.global_vector(CATALOGUE, self.car_ptrs)
        for va in LISTS.values():
            m.global_vector(va, [])
        for va in (OWNED, OWNED_AVAIL, REGISTRY, USED):
            m.global_vector(va, [])
        m.put_u32(G['cash'], 11000)
        m.uc.mem_write(G['cheat'], b'\0')
        self.dealer_pick = 0
        self.colours = [0] * 6
        gametype = m.fake_object({0x28: (lambda _m: 4, 0), 0x2c: (lambda _m: 0, 4)})
        dealer = m.fake_object({0x2c: (lambda _m: 0, 4), 0x9c: (lambda _m: self.dealer_pick, 0),
                                **{0x74 + 4 * k: (self._colour(k), 0) for k in range(6)}})
        used = m.fake_object({0x2c: (lambda _m: 0, 4)})
        m.casts.update({S_GAMETYPE: gametype, S_DEALER: dealer, S_USED: used})
        dialog_vtable = m.get_u32(m.fake_object({0x34: (lambda _m: 0, 0)}))

        def show(mm):                          # dialog constructor (thiscall, 2 args)
            self.dialogs.append(mm.arg(0))
            this = mm.uc.reg_read(UC_X86_REG_ECX)
            mm.put_u32(this, dialog_vtable)
            return this
        m.stubs[0x513440] = show
        m.pops[0x513440] = 8
        m.stubs[0x4aadb0] = lambda _m: self.calls.append('sound') or 0
        m.stubs[0x4fbc10] = lambda _m: self.calls.append('tracks') or 0

        def sprintf(mm):
            fmt = mm.cstr(mm.arg(1), 260)
            if fmt == '%s%s.crp':              # model file name: write the model only
                name = mm.cstr(mm.arg(3), 64).encode('latin1')
            elif '%' not in fmt:               # sprintf(dst, text): a plain copy
                name = fmt.encode('latin1')
            else:
                raise RuntimeError(f'sprintf format not stubbed: {fmt!r}')
            mm.uc.mem_write(mm.arg(0), name + b'\0')
            return len(name)
        m.stubs[0x5a0fbf] = sprintf
        m.stubs[0x59dc30] = lambda mm: 1 if mm.cstr(mm.arg(0), 64) in self.present else 0
        self.rows = []
        self.step = 0

    def _colour(self, k):
        return lambda _m: self.colours[k]

    def car_index(self, ptr):
        return (ptr - self.car_base) // CAR_RECORD

    def owned_dump(self):
        m = self.m
        out = []
        for car in m.vector_items(m.get_u32(OWNED)):
            img = bytearray(m.get_bytes(car, OWNED_CAR))
            img[0x758:0x760] = bytes(8)
            inst = m.vector_items(m.get_u32(car + 0x758))
            stock = m.vector_items(m.get_u32(car + 0x75c))
            inst_bytes = b''.join(m.get_bytes(p, PART_RECORD) for p in inst)
            out.append('{}:{:016x}:{}:{}:{:016x}'.format(
                m.i16(car + 4), fnv64(img),
                '/'.join(str(m.i16(p)) for p in inst) or '-',
                '/'.join(str(m.i16(p)) for p in stock) or '-', fnv64(inst_bytes)))
        return ';'.join(out) or '-'

    def garage_summary(self, full):
        dump = self.owned_dump()
        n = len(self.m.vector_items(self.m.get_u32(OWNED)))
        return dump if full else f'{n}:{fnv64(dump.encode()):016x}'

    def lists_dump(self):
        m = self.m
        parts = []
        for name, va in LISTS.items():
            parts.append(name + '=' + '/'.join(str(self.car_index(p)) for p in m.vector_items(m.get_u32(va))))
        owned = m.vector_items(m.get_u32(OWNED))
        avail = m.vector_items(m.get_u32(OWNED_AVAIL))
        parts.append('avail=' + '/'.join(str(owned.index(p)) for p in avail))
        return ' '.join(parts)

    def record(self, op, args, ret, lists=False):
        m = self.m
        self.rows.append([self.step, op, ','.join(str(a) for a in args) or '-', ret,
                          m.get_i32(G['cash'])[0], self.lists_dump() if lists else '-',
                          self.garage_summary(op == 'final') if op in ('buy', 'prize', 'rebuild', 'final') else '-',
                          ' '.join(self.calls) or '-'])
        self.calls.clear()
        self.step += 1

    def signed(self, v):
        return v - (1 << 32) if v & 0x80000000 else v

    # ops ----------------------------------------------------------------------
    def price(self, k, year):
        self.m.put_u32(G['year'], year)
        self.record('price', [k, year], self.signed(self.m.call(PRICE, self.car_ptrs[k])))

    def rebuild(self, year, cheat):
        m = self.m
        m.put_u32(G['year'], year)
        m.uc.mem_write(G['cheat'], bytes([cheat]))
        m.call(REBUILD, budget=20_000_000)
        self.record('rebuild', [year, cheat], 0, lists=True)

    def dealer(self):
        return [self.car_index(p) for p in self.m.vector_items(self.m.get_u32(LISTS['dealer']))]

    def buy(self, k, colours):
        self.dealer_pick = self.car_ptrs[k]
        self.colours = colours
        self.dialogs.clear()
        self.m.call(BUY, budget=20_000_000)
        ret = self.dialogs[0] if self.dialogs else 0
        self.record('buy', [k] + colours, ret)

    def prize(self, car_id, colours):
        self.m.call(PRIZE, car_id, *colours, budget=20_000_000)
        self.record('prize', [car_id] + colours, 0)

    def add_cash(self, v):
        self.m.call(ADD_CASH, v)
        self.record('addcash', [v], 0)


def play(g, seed):
    rng = random.Random(seed)
    n = len(g.cars)
    for k in range(n):
        first = struct.unpack_from('<h', g.cars[k], 0x1f8)[0]
        for year in sorted({1948, 1950, first - 1, first, first + 1, first + 3, first + 17, first + 50, 2000, 2002}):
            g.price(k, year)
    g.price(0, -5)
    g.buy(0, [1, 2, 3, 4, 5, 6])                 # 11 000 is not enough for most cars
    g.add_cash(2_000_000)
    for year in (1950, 1956, 1969, 1970, 1978, 1989, 1995, 2000):
        for cheat in (0, 1):
            g.rebuild(year, cheat)
        g.rebuild(year, 0)
        dealer = g.dealer()
        for _ in range(rng.randint(1, 4)):
            pick = rng.choice(dealer) if dealer and rng.random() < 0.9 else rng.randrange(n)
            g.buy(pick, [rng.getrandbits(32) for _ in range(6)])
        if rng.random() < 0.5:
            g.add_cash(rng.choice((50000, 250000, -3000)))
        if rng.random() < 0.3:
            cid = struct.unpack_from('<h', g.cars[rng.randrange(n)], 0)[0]
            g.prize(cid if rng.random() < 0.9 else 9999, [rng.getrandbits(32) for _ in range(6)])
    # Fill the garage to the 80-car limit.
    g.add_cash(900_000_000)
    g.rebuild(2000, 1)
    dealer = g.dealer()
    while len(g.m.vector_items(g.m.get_u32(OWNED))) < 81 and dealer:
        g.buy(rng.choice(dealer), [rng.getrandbits(32) for _ in range(6)])
        if g.rows[-1][3] == 61:
            break
    g.rebuild(1960, 0)
    g.record('final', [], 0)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--data', required=True, help="'synthetic' or a game dir with FEData/Data")
    ap.add_argument('--seed', type=int, default=3)
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
    models = [c[0x7f:0xb1].split(b'\0')[0].decode('latin1') for c in cars]
    rng = random.Random(a.seed)
    present = sorted({x for x in models if x and rng.random() < 0.85})
    g = Garage(m, cars, parts, set(present))
    play(g, a.seed)
    lines = ['# replay-career-garage.py: Car_DealerPrice 0x4ee220, Career_RebuildCarLists 0x4f3e40,',
             '# Garage_BuyDealerCar 0x501cf0, Garage_AddPrizeCar 0x502270 of nfs5.exe run in Unicorn.',
             f'# data: {"synthetic (no game data)" if a.data == "synthetic" else "nfs5.car + nfs5.prt of the user"}']
    lines += [f'car\t{k}\t{c.hex()}' for k, c in enumerate(cars)]
    lines += [f'part\t{k}\t{p.hex()}' for k, p in enumerate(parts)]
    lines += [f'present\t{x}' for x in present]
    lines.append('\t'.join(['step', 'op', 'args', 'ret', 'cash', 'lists', 'garage', 'calls']))
    lines += ['\t'.join(str(v) for v in r) for r in g.rows]
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{len(g.rows)} ops, {len(m.vector_items(m.get_u32(OWNED)))} owned cars -> {a.out}')


if __name__ == '__main__':
    main()
