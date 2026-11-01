"""Record the original used-car market of nfs5.exe in Unicorn.

Runs unchanged (career_harness.CareerMachine, the dealer setup of replay-career-garage.py):

- UsedMarket_Refresh 0x4f7db0(year): game RNG seeded with the clock value (`time` 0x5a17eb
  stubbed to return the op's seed; seeding 0x538110), UsedMarket_Evaluate 0x4f7e90 (offers on
  listed cars, sold market cars), removal of sold market cars (destructor 0x4ee450),
  repricing (Car_MarketValue 0x4f8e50 with OwnedCar_Condition 0x4ef960 / 0x4ef580 and
  Part_Value 0x518220), UsedMarket_Generate 0x4f80c0 over the model list 0x6919f0 of
  Career_RebuildCarLists 0x4f3e40, UsedCar_Create 0x4f82c0 (colours, the CRT rand 0x5a17c9
  for the decal, mileage and race tables, UsedCar_RandomParts 0x4f8590 with
  Parts_BuildMarket 0x517d10, condition tables, owned-car constructor 0x4ee3b0);
- Car_MarketValue 0x4f8e50 of garage cars; UsedMarket_ListCar 0x4f91c0, UsedMarket_Unlist
  0x4f9340, UsedMarket_DeclineOffer 0x4f9720, Garage_SellCar 0x502190,
  Garage_BuyUsedCar 0x501f40 (the `UsedCar.Car` accessor's current car 0x4f5a20 stubbed to the
  op's market index), Garage_BuyDealerCar 0x501cf0 to fill the garage.

The CRT rand is stubbed with the MSVC generator (state * 214013 + 2531011, bits 16..30)
from a recorded state (it lives in the per-thread data the harness does not provide); the
decal count [0x6115f8] is set to 7.

    PYTHON scripts/research/replay-career-market.py --game-dir local/game --data synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-game/tests/fixtures/career-market-synthetic.tsv
    PYTHON scripts/research/replay-career-market.py --game-dir local/game --data local/game \
        --out local/fixtures/career-market-car.tsv

Format: `car`, `part`, `present` lines as replay-career-garage.py, then op rows: step, op,
args, ret, cash, rng (the six game-RNG words and the CRT state, hex), market (count:FNV of
the per-slot dump), garage (count:FNV of the per-car dump), last (the dump of the slot or car
the op touched, or '-'). A slot/car dump is `catid:FNV(0x760 image, vector pointers
zeroed):installed ids/conditions:stock ids`.
"""
import argparse
import importlib.util
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from career_harness import (CAR_RECORD, OWNED_CAR, PART_RECORD, CareerMachine, fnv64,  # noqa: E402
                            records, synthetic_catalogue, synthetic_parts)
from vehicle_harness import load_module  # noqa: E402

_spec = importlib.util.spec_from_file_location('garage_rec', Path(__file__).with_name('replay-career-garage.py'))
garage_rec = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(garage_rec)

RNG = 0x5de910
MARKET, MARKET_YEAR, DECALS = 0x691b88, 0x691b58, 0x6115f8
REFRESH, EVALUATE, VALUE = 0x4f7db0, 0x4f7e90, 0x4f8e50
LIST, UNLIST, DECLINE, SELL, BUY_USED = 0x4f91c0, 0x4f9340, 0x4f9720, 0x502190, 0x501f40
TIME, CRT_RAND, USED_CURRENT = 0x5a17eb, 0x5a17c9, 0x4f5a20


class Market(garage_rec.Garage):
    def __init__(self, m, cars, parts, present):
        super().__init__(m, cars, parts, present)
        m.global_vector(MARKET, [])
        m.global_vector(0x691b90, [])
        m.put_u32(DECALS, 7)
        m.put_u32(MARKET_YEAR, 1950)
        self.seed = 0
        self.crt = 1
        self.used_pick = 0
        m.stubs[TIME] = lambda _m: self.seed
        m.stubs[CRT_RAND] = self._crt_rand
        m.stubs[USED_CURRENT] = lambda _m: self.used_pick
        self.mrows = []
        self.mstep = 0

    def _crt_rand(self, _m):
        self.crt = (self.crt * 214013 + 2531011) & 0xffffffff
        return (self.crt >> 16) & 0x7fff

    def car_dump(self, car):
        m = self.m
        img = bytearray(m.get_bytes(car, OWNED_CAR))
        img[0x758:0x760] = bytes(8)
        inst = m.vector_items(m.get_u32(car + 0x758))
        stock = m.vector_items(m.get_u32(car + 0x75c))
        return '{}:{:016x}:{}:{}'.format(
            m.i16(car + 4), fnv64(img),
            '/'.join(f'{m.i16(p)}={m.get(p + 0x164)[0]!r}' for p in inst) or '-',
            '/'.join(str(m.i16(p)) for p in stock) or '-')

    def summary(self, cars):
        d = ';'.join(self.car_dump(c) for c in cars) or '-'
        return f'{len(cars)}:{fnv64(d.encode()):016x}'

    def mrecord(self, op, args, ret, last=None):
        m = self.m
        rng = ','.join(f'{m.get_u32(RNG + 4 * k):08x}' for k in range(6)) + f',{self.crt:08x}'
        market = m.vector_items(m.get_u32(MARKET))
        owned = m.vector_items(m.get_u32(garage_rec.OWNED))
        self.mrows.append([self.mstep, op, ','.join(str(a) for a in args) or '-', ret,
                           m.get_i32(garage_rec.G['cash'])[0], rng, self.summary(market),
                           self.summary(owned), self.car_dump(last) if last else '-'])
        self.mstep += 1

    def market_cars(self):
        return self.m.vector_items(self.m.get_u32(MARKET))

    def owned_cars(self):
        return self.m.vector_items(self.m.get_u32(garage_rec.OWNED))

    # ops ------------------------------------------------------------------
    def m_rebuild(self, year):
        self.m.put_u32(garage_rec.G['year'], year)
        self.m.call(garage_rec.REBUILD, budget=20_000_000)
        self.mrecord('rebuild', [year], 0)

    def m_refresh(self, year, seed):
        self.seed = seed
        self.m.call(REFRESH, year, budget=200_000_000)
        self.mrecord('refresh', [year, seed], 0)

    def m_value(self, g):
        car = self.owned_cars()[g]
        r = self.m.call(VALUE, car, budget=20_000_000)
        self.mrecord('value', [g], r - (1 << 32) if r & 0x80000000 else r, last=car)

    def m_buy_dealer(self, k):
        self.dealer_pick = self.car_ptrs[k]
        self.colours = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]
        self.dialogs.clear()
        self.m.call(garage_rec.BUY, budget=20_000_000)
        self.mrecord('buydealer', [k], self.dialogs[0] if self.dialogs else 0)

    def m_list(self, g, asking):
        car = self.owned_cars()[g]
        self.m.call(LIST, car, asking, budget=20_000_000)
        self.mrecord('list', [g, asking], 0, last=car)

    def m_unlist(self, g):
        car = self.owned_cars()[g]
        self.m.call(UNLIST, car, budget=20_000_000)
        self.mrecord('unlist', [g], 0, last=car)

    def m_decline(self, g):
        car = self.owned_cars()[g]
        self.m.call(DECLINE, car)
        self.mrecord('decline', [g], 0, last=car)

    def m_sell(self, g):
        self.m.call(SELL, self.owned_cars()[g], budget=20_000_000)
        self.mrecord('sell', [g], 0)

    def m_buy_used(self, k):
        self.used_pick = self.market_cars()[k]
        self.dialogs.clear()
        self.m.call(BUY_USED, budget=20_000_000)
        self.mrecord('buyused', [k], self.dialogs[0] if self.dialogs else 0, last=self.used_pick)


def play(mk, seed):
    rng = random.Random(seed)
    mk.crt = rng.getrandbits(32)
    mk.mrecord('crt', [mk.crt], 0)
    mk.m.put_u32(garage_rec.G['cash'], 3_000_000)
    mk.mrecord('cash', [3_000_000], 0)
    years = [1950, 1953, 1956, 1960, 1964, 1968, 1970, 1973, 1976, 1980, 1984, 1988, 1990, 1993, 1996, 1998, 2000]
    for year in years:
        mk.m_rebuild(year)
        dealer = mk.dealer()
        for _ in range(rng.randint(0, 2)):
            if dealer:
                mk.m_buy_dealer(rng.choice(dealer))
        for _ in range(rng.randint(1, 3)):
            mk.m_refresh(year, rng.getrandbits(31))
            owned = mk.owned_cars()
            for g in range(len(owned)):
                if rng.random() < 0.3:
                    mk.m_value(g)
            owned = mk.owned_cars()
            for g in reversed(range(len(owned))):
                if mk.m.get_bytes(owned[g] + 0x692, 1)[0] and rng.random() < 0.4:
                    if rng.random() < 0.75:
                        mk.m_sell(g)
                    else:
                        mk.m_decline(g)
            owned = mk.owned_cars()
            market = [k for k, c in enumerate(mk.market_cars()) if mk.m.get_bytes(c + 0x6a0, 1)[0] == 0]
            r = rng.random()
            if owned and r < 0.35:
                g = rng.randrange(len(owned))
                if mk.m.get_bytes(owned[g] + 0x691, 1)[0] == 0:
                    mk.m_list(g, rng.choice((1000, 9000, 25000, 60000, 150000, 400000)))
            elif owned and r < 0.5:
                listed = [g for g, c in enumerate(owned) if mk.m.get_bytes(c + 0x692, 1)[0]]
                if listed:
                    g = rng.choice(listed)
                    if rng.random() < 0.7:
                        mk.m_sell(g)
                    else:
                        mk.m_decline(g)
            elif owned and r < 0.55:
                listed = [g for g, c in enumerate(owned) if mk.m.get_bytes(c + 0x691, 1)[0]]
                if listed:
                    mk.m_unlist(rng.choice(listed))
            elif market and r < 0.8:
                mk.m_buy_used(rng.choice(market))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--data', required=True, help="'synthetic' or a game dir with FEData/Data")
    ap.add_argument('--seed', type=int, default=9)
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
    present = sorted({x for x in models if x and rng.random() < 0.9})
    mk = Market(m, cars, parts, set(present))
    play(mk, a.seed)
    lines = ['# replay-career-market.py: UsedMarket_Refresh 0x4f7db0 and the used-car market of nfs5.exe',
             '# run in Unicorn.',
             f'# data: {"synthetic (no game data)" if a.data == "synthetic" else "nfs5.car + nfs5.prt of the user"}']
    lines += [f'car\t{k}\t{c.hex()}' for k, c in enumerate(cars)]
    lines += [f'part\t{k}\t{q.hex()}' for k, q in enumerate(parts)]
    lines += [f'present\t{x}' for x in present]
    lines.append('\t'.join(['step', 'op', 'args', 'ret', 'cash', 'rng', 'market', 'garage', 'last']))
    lines += ['\t'.join(str(v) for v in r) for r in mk.mrows]
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{len(mk.mrows)} ops, market {len(mk.market_cars())}, garage {len(mk.owned_cars())} -> {a.out}')


if __name__ == '__main__':
    main()
