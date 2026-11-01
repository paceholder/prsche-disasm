"""Record the original career save sections PlayerInfo, UsedCarMarket and PartsMarket.

A career state is built with the original functions (replay-career-market.py: rebuilds,
dealer purchases, market refreshes, listings, used-car purchases; spare parts bought with
0x5183c0 and installed with Part_Install 0x518510, wear written into installed copies), then
the original writers run: Player_SaveSection 0x5018b0 (cars through OwnedCar_Write 0x4ee8d0),
UsedMarket_SaveSection 0x4f7a10, Parts_SaveSection 0x517a30. The written bytes are loaded
back with the original readers Player_LoadSection 0x501610 (OwnedCar_Read 0x4ee6a0 on cars
from the default constructor 0x4ee5c0), UsedMarket_LoadSection 0x4f7750,
Parts_LoadSection 0x517810, then Garage_RelistCars 0x501a70 (Save_ReadProfile 0x4de9b0 runs it
after the sections). The loaded state is dumped and saved again.

    PYTHON scripts/research/replay-career-save.py --game-dir local/game --data synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-game/tests/fixtures/career-save-synthetic.tsv
    PYTHON scripts/research/replay-career-save.py --game-dir local/game --data local/game \
        --out local/fixtures/career-save-car.tsv

Format: `part` table lines (the parts table the readers look ids up in), then `saved <section> <hex>` (first save),
`loaded <key> <value>` (state after loading: cash, driver, names, flags, ints, garage,
market, spares, market year) and `resaved <section> <bytes>:<FNV-1a 64>`.
"""
import argparse
import importlib.util
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from career_harness import (CAR_RECORD, PART_RECORD, CareerMachine, fnv64, records,  # noqa: E402
                            synthetic_catalogue, synthetic_parts)
from vehicle_harness import load_module  # noqa: E402

_spec = importlib.util.spec_from_file_location('market_rec', Path(__file__).with_name('replay-career-market.py'))
market_rec = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(market_rec)
garage_rec = market_rec.garage_rec

PLAYER_SAVE, PLAYER_LOAD = 0x5018b0, 0x501610
MARKET_SAVE, MARKET_LOAD = 0x4f7a10, 0x4f7750
PARTS_SAVE, PARTS_LOAD = 0x517a30, 0x517810
RELIST = 0x501a70
BUY_PART, INSTALL = 0x5183c0, 0x518510
SPARES = 0x69a0ec
G = {'cash': 0x691d90, 'driver': 0x5db748, 'name': 0x691dc8, 'name2': 0x691de8, 'started': 0x691dd5,
     'flag_d94': 0x691d94, 'flag_df5': 0x691df5, 'ints': 0x691d98, 'year': 0x5db764}
BUF = 0x7000000            # mapped by main()
BUF_SIZE = 0x40000


def build(mk, seed):
    rng = random.Random(seed)
    m = mk.m
    mk.crt = rng.getrandbits(32)
    m.put_u32(garage_rec.G['cash'], 3_000_000)
    for year in (1956, 1968, 1985, 1995):
        mk.m_rebuild(year)
        dealer = mk.dealer()
        for _ in range(2):
            if dealer:
                mk.m_buy_dealer(rng.choice(dealer))
        mk.m_refresh(year, rng.getrandbits(31))
        market = [k for k, c in enumerate(mk.market_cars()) if m.get_bytes(c + 0x6a0, 1)[0] == 0]
        if market:
            mk.m_buy_used(rng.choice(market))
    owned = mk.owned_cars()
    for g in rng.sample(range(len(owned)), min(3, len(owned))):
        mk.m_list(g, rng.choice((5000, 30000, 90000)))
    mk.m_refresh(1996, rng.getrandbits(31))
    # Spare parts and installs (unsorted installed lists), wear on installed copies.
    for _ in range(12):
        k = rng.randrange(len(mk.parts))
        m.call(BUY_PART, mk.part_base_addr + PART_RECORD * k, 1996, garage_rec.ADD_CASH, budget=20_000_000)
    owned = mk.owned_cars()
    for _ in range(6):
        spares = m.vector_items(m.get_u32(SPARES))
        if spares and owned:
            m.call(INSTALL, rng.choice(spares), rng.choice(owned), budget=20_000_000)
    for car in owned:
        for p in m.vector_items(m.get_u32(car + 0x758)):
            if rng.random() < 0.5:
                m.put(p + 0x164, rng.choice((3.0, 17.5, 42.0)))
    m.put_i32(G['driver'], 3)
    m.put_str(G['name'], 'Racer One', 32)
    m.put_str(G['name2'], 'second', 13)   # 0x691de8..0x691df4 (0x691df5 is a flag, 0x691df8 the garage)
    for k, v in (('started', 1), ('flag_d94', 1), ('flag_df5', 0)):
        m.uc.mem_write(G[k], bytes([v]))
    m.put_i32(G['ints'], *[rng.randint(-5, 900) for _ in range(8)])


def save(m, va):
    m.put_u32(BUF - 0x10, BUF)
    m.uc.mem_write(BUF, bytes(BUF_SIZE))
    ok = m.call(va, BUF - 0x10, budget=50_000_000) & 0xff
    if not ok:
        raise RuntimeError(f'save {va:x} failed')
    return m.get_bytes(BUF, m.get_u32(BUF - 0x10) - BUF)


def load(m, va, data):
    m.uc.mem_write(BUF, data + bytes(16))
    m.put_u32(BUF - 0x10, BUF)
    ok = m.call(va, BUF - 0x10, budget=50_000_000) & 0xff
    if not ok:
        raise RuntimeError(f'load {va:x} failed')
    return m.get_u32(BUF - 0x10) - BUF


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--data', required=True, help="'synthetic' or a game dir with FEData/Data")
    ap.add_argument('--seed', type=int, default=4)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    module, data = load_module('nfs5.exe', a.game_dir)
    m = CareerMachine(module, data)
    m.uc.mem_map(BUF - 0x1000, BUF_SIZE + 0x2000)
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
    mk = market_rec.Market(m, cars, parts, set(present))
    mk.part_base_addr = m.get_u32(0x628f84)
    m.global_vector(SPARES, [])
    m.global_vector(0x69a0e8, [])
    for va in (0x691b94, 0x691b8c):          # further market vectors freed by the market loader
        m.global_vector(va, [])
    build(mk, a.seed)
    sections = [('PlayerInfo', PLAYER_SAVE, PLAYER_LOAD), ('UsedCarMarket', MARKET_SAVE, MARKET_LOAD),
                ('PartsMarket', PARTS_SAVE, PARTS_LOAD)]
    saved = {name: save(m, sv) for name, sv, _ in sections}
    for name, _, ld in sections:
        used = load(m, ld, saved[name])
        if used != len(saved[name]):
            raise RuntimeError(f'{name}: loaded {used} of {len(saved[name])} bytes')
    m.call(RELIST, budget=20_000_000)
    loaded = [
        ('cash', m.get_i32(G['cash'])[0]), ('driver', m.get_i32(G['driver'])[0]),
        ('name', m.cstr(G['name'], 32)), ('name2', m.cstr(G['name2'], 13)),
        ('flags', ','.join(str(m.get_bytes(G[k], 1)[0]) for k in ('started', 'flag_d94', 'flag_df5'))),
        ('ints', ','.join(str(v) for v in m.get_i32(G['ints'], 8))),
        ('market_year', m.get_i32(market_rec.MARKET_YEAR)[0]),
        ('garage', ';'.join(mk.car_dump(c) for c in mk.owned_cars()) or '-'),
        ('market', ';'.join(mk.car_dump(c) for c in mk.market_cars()) or '-'),
        ('spares', '/'.join(f'{m.i16(p)}:{m.get(p + 0x164)[0]!r}' for p in m.vector_items(m.get_u32(SPARES))) or '-'),
    ]
    resaved = {name: save(m, sv) for name, sv, _ in sections}
    lines = ['# replay-career-save.py: PlayerInfo 0x5018b0/0x501610, UsedCarMarket 0x4f7a10/0x4f7750,',
             '# PartsMarket 0x517a30/0x517810 and Garage_RelistCars 0x501a70 of nfs5.exe run in Unicorn.',
             f'# data: {"synthetic (no game data)" if a.data == "synthetic" else "nfs5.car + nfs5.prt of the user"}']
    lines += [f'part\t{k}\t{q.hex()}' for k, q in enumerate(parts)]
    lines += [f'saved\t{n}\t{b.hex()}' for n, b in saved.items()]
    lines += [f'loaded\t{k}\t{v}' for k, v in loaded]
    lines += [f'resaved\t{n}\t{len(b)}:{fnv64(b):016x}' for n, b in resaved.items()]
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{len(mk.owned_cars())} cars, {len(mk.market_cars())} market slots, sections '
          f'{[len(b) for b in saved.values()]} -> {a.out}')


if __name__ == '__main__':
    main()
