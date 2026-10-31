"""Record the original Evolution career bookkeeping by executing nfs5.exe in Unicorn.

A scripted career (seeded driver below) is played through the original functions, run
unchanged (vehicle_harness.Machine, x87 control word 0x27f):

- Race_ApplyResults 0x4edf00, game type 4 (with Tourn_RecordRaceResult 0x503960,
  Tourn_Standing 0x503660 + sorter 0x502880, Tourn_PlayerRaceWinnings 0x505740,
  Tourn_PlayerRaceBonus 0x5057a0, Tourn_RacesLeft 0x5056d0, Tourn_PlayerRank 0x505710,
  Tourn_OverallPrize 0x505770, Player_AddCash 0x5012e0, Tourn_ClearRunning 0x503a50,
  Tourn_OnTournamentDone 0x503ad0, Tourn_ResetResults 0x503e40); also with the abort flag
  [0x657e34] and the debug completions [0x657e20] (0x504370) / [0x657e21] (0x504450);
- Tourn_SelectList 0x505010, Tourn_CarAllowed 0x504a70, Tourn_OtherPartlyPlayed 0x504b60,
  Tourn_ResetAbandoned 0x504bd0, Tourn_Standing 0x503660, Era_IsComplete 0x503ed0,
  Career_HighestCompletedEra 0x504800, Era_FindType2Record 0x504990, Career_SetYear 0x504170,
  Player_AddCash 0x5012e0;
- the Evolution part of FE_LaunchRace 0x4d5730 (after its car and dialog checks), glued here
  from its original callees: Tourn_NumRaces 0x503370 == Tourn_RacesLeftById 0x504300 ->
  Player_GetCash 0x5012d0 < Tourn_EntryFee 0x503430 ? refuse (string 857) :
  Player_AddCash(-fee); then Tourn_SetRunning 0x503940.

After every operation the original Tourn_SaveSection 0x502ae0 writes the `Tournament` save
payload; the row stores its FNV-1a 64 hash and the career globals, so the Rust replay checks
the whole tournament table bit for bit.

Stubbed (recorded as calls): track records 0x4fb100 / 0x4fafc0, Career_RebuildCarLists
0x4f3e40 ('rebuild'), the FE race-index vector refresh 0x5041b0 ('races'), the debug print
0x504a40. Tourn_BuildEraLists 0x502ba0 is not run (heap/FE); its race-count clamp to 10 is
applied by the recorder when the table is loaded, as the original does on load.

Tables: --table synthetic (generated below, no game data; fixture committed to the engine
repo) or --table <path to nfs5.trn> (the user's file; fixture goes to local/fixtures).

    PYTHON scripts/research/replay-career-progress.py --game-dir local/game --table synthetic \
        --out iterations/012-campaign-fidelity/crates/nfs-game/tests/fixtures/career-progress-synthetic.tsv
    PYTHON scripts/research/replay-career-progress.py --game-dir local/game \
        --table local/game/FEData/Data/nfs5.trn --out local/fixtures/career-progress-trn.tsv

File format (TSV, '#' comments): `size <table bytes>`, `trn <offset hex> <hex bytes>` lines (non-zero runs of the
table after the clamp), then one `op` row per operation:
step, op, args (comma list), ret, year, era, complete, cur, next, run_race, run_tourn,
era_changed, cash, odometer, wins, calls (stub calls of the op), payload FNV-1a 64 (hex).
Meanings: docs/re/career.md in the workspace root.
"""
import argparse
import random
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from vehicle_harness import Machine, load_module  # noqa: E402

REC = 0xd40
RACE = 0xa4
TRN = 0x3000000            # table (mapped 0x40000)
SAVE = 0x3040000           # save payload buffer (mapped 0x40000)
SCRATCH = 0x3080000        # player car object, FE vector (mapped 0x10000)
PLAYER_CAR = SCRATCH
FE_VECTOR = SCRATCH + 0x1000
FE_VECTOR_DATA = SCRATCH + 0x2000

G = {
    'year': 0x5db764, 'run_race': 0x5db768, 'run_tourn': 0x5db76c, 'era': 0x691e50,
    'complete': 0x691e54, 'records': 0x691e58, 'count': 0x691e64, 'blob': 0x691e68,
    'cur': 0x691e6c, 'size': 0x691e74, 'next': 0x691e7c, 'era_changed': 0x691e81,
    'cash': 0x691d90, 'game_type': 0x690ad4, 'player_index': 0x690ad8, 'player_car': 0x690adc,
    'abort': 0x657e34, 'cheat_tourn': 0x657e20, 'cheat_era': 0x657e21, 'linear': 0x657494,
    'cars': 0x5e8ae8, 'fe_vector': 0x691e1c,
}
RESULTS = 0x5e77a0         # per car: place +0, time +4, bonus-blocking flag byte at -0x14
RESULT_STRIDE = 0x258

APPLY, SAVE_SECTION = 0x4edf00, 0x502ae0
SELECT, CAR_ALLOWED, OTHER_PARTLY, RESET_ABANDONED = 0x505010, 0x504a70, 0x504b60, 0x504bd0
STANDING, ERA_COMPLETE, HIGHEST_ERA, TYPE2, SET_YEAR = 0x503660, 0x503ed0, 0x504800, 0x504990, 0x504170
ADD_CASH, GET_CASH, NUM_RACES, RACES_LEFT_BY_ID, ENTRY_FEE, SET_RUNNING, CURRENT_ID = (
    0x5012e0, 0x5012d0, 0x503370, 0x504300, 0x503430, 0x503940, 0x503310)
STUBS = {0x4fb100: 'records', 0x4fafc0: 'records', 0x4f3e40: 'rebuild', 0x5041b0: 'races',
         0x504a40: 'print'}

ERAS = (0x209, 0x20a, 0x20b)
CLUBS = (0x20c, 0x20d, 0x20e)


def fnv64(data):
    h = 0xcbf29ce484222325
    for b in data:
        h = ((h ^ b) * 0x100000001b3) & 0xffffffffffffffff
    return h


# ---------------------------------------------------------------------------
# Synthetic table: the layout of nfs5.trn (docs/re/career.md), invented contents.
def synthetic_table(seed=5):
    rng = random.Random(seed)
    recs = []
    next_id = [1]

    def rec(era, year, kind=0, races=None, sort=0, dup_id=None):
        r = bytearray(REC)
        tid = dup_id if dup_id is not None else next_id[0]
        next_id[0] += 1 if dup_id is None else 0
        struct.pack_into('<h', r, 0x000, tid)
        struct.pack_into('<i', r, 0x004, era)
        struct.pack_into('<h', r, 0x008, year)
        n_tracks = rng.randint(1, 8)
        struct.pack_into('<h', r, 0x00a, n_tracks)
        for k in range(n_tracks):
            struct.pack_into('<h', r, 0x00c + 2 * k, rng.choice((8, 9, 10, 11, 13, 14, 16, 34)))
        for o, base in ((0x20, 1000), (0x24, 1100), (0x28, 1200), (0x2c, 1300)):
            struct.pack_into('<i', r, o, base + tid)
        n_cars = rng.randint(0, 20)
        for k in range(n_cars):
            struct.pack_into('<h', r, 0x030 + 2 * k, rng.randint(1, 60))
        struct.pack_into('<h', r, 0x0da, n_cars)
        struct.pack_into('<i', r, 0x0dc, 0 if kind == 2 else rng.choice((0, 0, 500, 1000, 3500, 20000, 45000)))
        struct.pack_into('<h', r, 0x0e0, rng.randint(0, 3))
        struct.pack_into('<h', r, 0x0e4, kind)
        r[0x0e8] = sort
        top = rng.choice((5000, 11000, 60000, 175000))
        for k in range(8):
            struct.pack_into('<i', r, 0x0ec + 4 * k, 0 if kind == 2 else top * (8 - k) // 8 + rng.randint(0, 99))
        struct.pack_into('<h', r, 0x10c, rng.choice((0, 168, 169, 170)))
        n = races if races is not None else rng.randint(1, 6)
        struct.pack_into('<h', r, 0x39c, n)
        for k in range(min(n, 12)):
            if 0x3a0 + (k + 1) * RACE > 0xd3c:
                break
            b = 0x3a0 + k * RACE
            struct.pack_into('<h', r, b + 0x00, rng.choice((8, 9, 10, 11, 12, 13, 14, 16, 18, 25, 26, 29, 30, 32, 34)))
            struct.pack_into('<h', r, b + 0x02, rng.choice((0, 0, 2, 3, 5)))
            r[b + 0x04] = rng.randint(0, 1)
            prize = rng.choice((0, 2000, 4750, 7500, 40000))
            for p in range(8):
                struct.pack_into('<i', r, b + 0x08 + 4 * p, prize * (8 - p) // 8 + rng.randint(0, 7))
            struct.pack_into('<i', r, b + 0x28, rng.choice((0, 750, 851, 1001, 1250, 7503, 33333)))
            struct.pack_into('<f', r, b + 0x2c, round(rng.uniform(0.45, 1.0), 2))
        recs.append(r)

    for era, year, n in ((0x209, 1950, 4), (0x20a, 1970, 4), (0x20b, 1989, 5)):
        for k in range(n):
            rec(era, year, races=12 if (era, k) == (0x20a, 1) else None, sort=1 if k == 2 else 0)
        rec(era, year, kind=2, races=1)
    for club in CLUBS:
        for k in range(3):
            rec(club, 0, sort=k % 2)
    rec(0x20d, 0, dup_id=next_id[0] - 1)     # duplicate id: lookups find the first record
    rec(0x210, 0)                            # list id outside the known lists
    rec(0x209, 1950, kind=2, races=2)        # a second type-2 record in Classic
    return b''.join(recs)


def clamp_races(table):
    t = bytearray(table)
    for i in range(len(t) // REC):
        o = i * REC + 0x39c
        if struct.unpack_from('<h', t, o)[0] > 10:
            struct.pack_into('<h', t, o, 10)
    return bytes(t)


# ---------------------------------------------------------------------------
class Career:
    def __init__(self, m, table, calls):
        self.m, self.calls = m, calls
        self.n = len(table) // REC
        m.uc.mem_write(TRN, table)
        for name, value in (('records', TRN), ('blob', TRN), ('count', self.n), ('size', len(table)),
                            ('year', 1950), ('era', 0x209), ('complete', 0), ('cur', 0), ('next', 0),
                            ('run_race', -1), ('run_tourn', -1), ('cash', 11000),
                            ('game_type', 4), ('player_index', 0), ('player_car', PLAYER_CAR),
                            ('abort', 0), ('cheat_tourn', 0), ('cheat_era', 0), ('linear', 0),
                            ('cars', 0), ('fe_vector', FE_VECTOR)):
            m.put_u32(G[name], value)
        m.uc.mem_write(G['era_changed'], b'\0')
        m.uc.mem_write(PLAYER_CAR, bytes(0x760))
        m.put_u32(FE_VECTOR, FE_VECTOR_DATA)
        m.put_u32(FE_VECTOR + 4, FE_VECTOR_DATA)
        m.put_u32(FE_VECTOR + 8, FE_VECTOR_DATA + 0x1000)
        # Era id lists of Tourn_BuildEraLists 0x502ba0 (std::vector<int> {begin, end, cap}):
        # ids of the records of the list, table order, type-2 records skipped. Read by
        # Era_CheatComplete 0x504450.
        lists = SCRATCH + 0x4000
        for k, (era, vec) in enumerate(((0x209, 0x691e34), (0x20a, 0x691e30), (0x20b, 0x691e18),
                                        (0x20c, 0x691e5c), (0x20d, 0x691e3c), (0x20e, 0x691e60))):
            ids = [struct.unpack_from('<h', table, i * REC)[0] for i in range(self.n)
                   if struct.unpack_from('<i', table, i * REC + 4)[0] == era
                   and struct.unpack_from('<h', table, i * REC + 0xe4)[0] != 2]
            hdr, data = lists + 0x400 * k, lists + 0x400 * k + 0x10
            m.put_u32(vec, hdr)
            m.put_u32(hdr, data)
            m.put_u32(hdr + 4, data + 4 * len(ids))
            m.put_u32(hdr + 8, data + 4 * len(ids))
            for j, v in enumerate(ids):
                m.put_i32(data + 4 * j, v)
        self.rows = []
        self.step = 0

    def i32(self, name):
        return self.m.get_i32(G[name])[0]

    def rec_i16(self, idx, off):
        return struct.unpack('<h', self.m.get_bytes(TRN + idx * REC + off, 2))[0]

    def rec_i32(self, idx, off):
        return self.m.get_i32(TRN + idx * REC + off)[0]

    def payload(self):
        m = self.m
        m.put_u32(SCRATCH + 0x3000, SAVE)
        m.call(SAVE_SECTION, SCRATCH + 0x3000)
        end = m.get_u32(SCRATCH + 0x3000)
        return m.get_bytes(SAVE, end - SAVE)

    def record(self, op, args, ret):
        m = self.m
        self.rows.append([
            self.step, op, ','.join(str(a) for a in args) or '-', ret,
            self.i32('year'), self.i32('era'), self.i32('complete'), self.i32('cur'),
            self.i32('next'), self.i32('run_race'), self.i32('run_tourn'),
            m.get_bytes(G['era_changed'], 1)[0], self.i32('cash'),
            m.get_i32(PLAYER_CAR + 0x698)[0], m.get_i32(PLAYER_CAR + 0x69c)[0],
            ' '.join(self.calls) or '-', f'{fnv64(self.payload()):016x}'])
        self.calls.clear()
        self.step += 1

    def call(self, op, va, *args, signed=True):
        self.calls.clear()
        ret = self.m.call(va, *args)
        if signed and ret & 0x80000000:
            ret -= 1 << 32
        return ret

    # operations ----------------------------------------------------------
    def select(self, lst):
        self.call('select', SELECT, lst)
        self.record('select', [lst], 0)

    def car_allowed(self, car):
        tid = self.call('id', CURRENT_ID) if 0 <= self.i32('cur') < self.n else 0
        ret = self.call('allowed', CAR_ALLOWED, tid, car) & 0xff
        self.record('allowed', [tid, car], ret)
        return ret

    def other_partly(self):
        ret = self.call('partly', OTHER_PARTLY) & 0xff
        self.record('partly', [], ret)
        return ret

    def abandon(self):
        self.call('abandon', RESET_ABANDONED)
        self.record('abandon', [], 0)

    def launch(self):
        """FE_LaunchRace 0x4d5730, Evolution tail (after its car/dialog checks)."""
        self.calls.clear()
        m = self.m
        tid = m.call(CURRENT_ID)
        tid -= (1 << 32) if tid & 0x80000000 else 0
        ret = 1
        if m.call(NUM_RACES, tid) == m.call(RACES_LEFT_BY_ID, tid):
            cash = m.call(GET_CASH)
            cash -= (1 << 32) if cash & 0x80000000 else 0
            fee = m.call(ENTRY_FEE, tid)
            fee -= (1 << 32) if fee & 0x80000000 else 0
            if cash < fee:
                ret = 857
            else:
                m.call(ADD_CASH, -fee)
        if ret == 1:
            m.call(SET_RUNNING)
        self.record('launch', [], ret)
        return ret

    def add_cash(self, v):
        self.call('cash', ADD_CASH, v)
        self.record('addcash', [v], 0)

    def race(self, results, flag=None):
        """results: [(place, time, bonus_blocked)] per car slot; flag: abort/cheat_tourn/cheat_era."""
        m = self.m
        m.uc.mem_write(RESULTS - 0x20, bytes(8 * RESULT_STRIDE + 0x20))
        for k, (place, time, blocked) in enumerate(results):
            b = RESULTS + k * RESULT_STRIDE
            m.put_i32(b, place)
            m.put_i32(b + 4, time)
            m.uc.mem_write(b - 0x14, bytes([blocked]))
        m.put_u32(G['cars'], len(results))
        for name in ('abort', 'cheat_tourn', 'cheat_era'):
            m.uc.mem_write(G[name], bytes([1 if flag == name else 0]))
        self.call('race', APPLY)
        for name in ('abort', 'cheat_tourn', 'cheat_era'):
            m.uc.mem_write(G[name], b'\0')
        args = [flag or 'race'] + [f'{p}:{t}:{b}' for p, t, b in results]
        self.record('race', args, 0)

    def standing(self, tid, slot):
        ret = self.call('standing', STANDING, tid, slot)
        self.record('standing', [tid, slot], ret)

    def era_query(self, era):
        a = self.call('q', ERA_COMPLETE, era) & 0xff
        b = self.call('q', HIGHEST_ERA)
        c = (self.call('q', TYPE2, era) - TRN) // REC
        self.record('eraquery', [era], f'{a}:{b}:{c}')

    def set_year(self, y):
        self.call('year', SET_YEAR, y)
        self.record('setyear', [y], 0)


def play(c, seed, max_steps):
    rng = random.Random(seed)
    while c.step < max_steps and not c.i32('complete'):
        lst = 0 if rng.random() < 0.8 else rng.randint(1, 3)
        c.select(lst)
        if c.i32('cur') < 0:
            continue
        cur = c.i32('cur')
        c.car_allowed(rng.choice([rng.randint(1, 60), c.rec_i16(cur, 0x30)]))
        if rng.random() < 0.3:
            c.standing(c.rec_i16(cur, 0), rng.randint(0, 7))
        if c.other_partly() and rng.random() < 0.7:
            c.abandon()
        races = c.rec_i16(cur, 0x39c)
        strong = rng.random() < 0.75
        for _ in range(max(races, 1) + 1):
            if c.launch() == 857:
                c.add_cash(c.rec_i32(cur, 0xdc) + rng.randint(0, 3000))
                if c.launch() == 857:
                    break
            n = 8 if rng.random() < 0.4 else rng.randint(1, 8)
            places = list(range(1, n + 1))
            rng.shuffle(places)
            if strong and n > 1 and rng.random() < 0.8:
                best = places.index(min(places[:4]) if rng.random() < 0.5 else 1)
                places[0], places[best] = places[best], places[0]
            times = [60000 + p * rng.randint(0, 900) + rng.choice((0, 0, 1)) for p in places]
            if rng.random() < 0.1:
                times = [70000] * n          # equal times: sort ties
            res = [(p, t, 1 if rng.random() < 0.3 else 0) for p, t in zip(places, times)]
            r = rng.random()
            flag = 'abort' if r < 0.03 else 'cheat_tourn' if r < 0.04 else 'cheat_era' if r < 0.045 else None
            c.race(res, flag)
            if c.i32('cur') != cur or c.i32('run_tourn') == -1 and c.i32('next') == 0:
                break
            if rng.random() < 0.05:
                break                        # walk away mid-tournament
        if rng.random() < 0.15:
            c.era_query(rng.choice(ERAS + (0x20c, 0x300)))
    c.era_query(0x20b)
    c.standing(c.rec_i16(0, 0), 0)
    for y in (1969, 1970, 1988, 1989, 2001):
        c.set_year(y)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--game-dir', type=Path)
    ap.add_argument('--table', required=True, help="'synthetic' or the path of nfs5.trn")
    ap.add_argument('--seeds', default='7', help='comma list; one career per seed, each from a fresh table')
    ap.add_argument('--steps', type=int, default=2500)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    module, data = load_module('nfs5.exe', a.game_dir)
    calls = []

    def stub(name):
        def h(_m):
            calls.append(name)
            return 0
        return h

    m = Machine(module, data, stubs={va: stub(n) for va, n in STUBS.items()})
    m.uc.mem_map(TRN, 0x90000)
    table = synthetic_table() if a.table == 'synthetic' else Path(a.table).read_bytes()
    table = clamp_races(table)
    rows = []
    seeds = [int(x) for x in a.seeds.split(',')]
    for seed in seeds:
        c = Career(m, table, calls)
        c.record('init', [seed], 0)
        play(c, seed, a.steps)
        rows += c.rows
        print(f'seed {seed}: {len(c.rows)} ops, year {c.i32("year")}, era {c.i32("era"):#x}, '
              f'complete {c.i32("complete")}')

    lines = [
        '# replay-career-progress.py: Race_ApplyResults 0x4edf00 (type 4) and the Tourn_* career',
        '# functions of nfs5.exe run unchanged in Unicorn; Tourn_SaveSection 0x502ae0 payload hashed.',
        f'# table: {"synthetic (no game data)" if a.table == "synthetic" else "nfs5.trn of the user"}; seeds {a.seeds}',
        '# an `init` op starts a new career on the pristine table (steps restart at 0)',
    ]
    lines.append(f'size\t{len(table)}')
    for off in range(0, len(table), 64):
        chunk = table[off:off + 64]
        if any(chunk):
            lines.append(f'trn\t{off:x}\t{chunk.hex()}')
    lines.append('\t'.join(['step', 'op', 'args', 'ret', 'year', 'era', 'complete', 'cur', 'next',
                            'run_race', 'run_tourn', 'era_changed', 'cash', 'odometer', 'wins',
                            'calls', 'payload_fnv']))
    for row in rows:
        lines.append('\t'.join(str(v) for v in row))
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'{len(rows)} ops -> {a.out}')


if __name__ == '__main__':
    main()
