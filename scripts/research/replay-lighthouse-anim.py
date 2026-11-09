"""Record the original animated-scenery player (WAnimBundle + CamAnim) by executing nfs5.exe.

The scene build 0x4879d0 offers the track articles to a chain of bundles; `WAnimBundle`
(vtable 0x5b46b8, ctor 0x477710) takes, in its slot 1 `0x4777f0`, every offered article that
has an `Anqt` entry and gives it a `CamAnim` player (vtable 0x5b460c):
`0x471490(count = Anqt +8, keys = Anqt data, 0, flags [0x5b45a4] = 1 (loop), 0x3dcccccd = 0.1 s
per key, drift 0)`, started at once through player slot 5 `0x4716e0` (start = the clock value
`*[[0x628938] + 4]`, i.e. `g_SchedStep` 0x606ac4). Per drawn object the bundle's slot 5
`0x4777b0(index, matrix)` calls player slot 1 `0x4715a0(matrix, 0)`: elapsed = `g_SchedStep -
start` (negative -> 0), `0x470e90` (key `trunc(elapsed / f32(0.1 / (1/64)))`, loop flag:
`% (count - 2)`) and `0x470f70` (linear key blend, quaternion matrix, scale rows, 4x4).

Executes unmodified: the bundle ctor 0x477710 (0x477ba0, 0x489160), slot 1 0x4777f0 with its
own vtable calls (slot 9 0x477d10, slot 10 0x477d40), the CRP entry lookups 0x59f7c0 /
0x59f5e0 / 0x59f870 on the CRP image in emulator memory, the player ctor 0x471490, start
0x4716e0, and per frame bundle slot 5 0x4777b0 -> 0x4715a0 -> 0x470e90 -> 0x470f70 (0x48a150,
0x48a1e0, 0x5329a0, 0x539cd0, 0x532360). The clock list comes from the real
`CamAnimList_Init 0x471700(&g_SchedStep, 1/64)` (as the track loader 0x4876e0 calls it).

Stubbed (declared): the heap (bump allocator for 0x59eff0 / 0x59ef90 / 0x59ecb0, free no-op);
`0x47edf0` (animdefs lookup of `Base +0x44` on [0x628ba0], thiscall, returns 0: the bundle then
skips its animdefs attribute copy, which does not feed the pose); `0x44ff60` (the bundle's
render object, thiscall, returns a dummy). The clock: the recorder writes `g_SchedStep`
before every call (the original's writer is `Sched_Step` 0x414ff0 in `Race_MainLoop`, not run).
The FPU control word is 0x027f (53-bit), as in every other recorder.

Cases
- synthetic (committed fixture): a hand-made CRP image with five articles (`Anqt` 7 keys with
  odd quaternions / positions / scales; `Anqt` 5 keys plus a `tr`; no `Anqt`; `Anqt` 3 keys;
  an empty slot), start steps 0 and 37, steps 0..119 and every 23rd to 3000, the steps around
  the wraps of the 7-key loop, 2^20 and 2^31 - 1; a 60 Hz and a 20 Hz frame
  clock (`g_SchedStep = floor(frame * 64 / fps)`) with one and two pose calls per frame, 6 s each.
- tracks (--tracks, local fixture, reads the game CRPs): every article of each track CRP with
  an `Anqt` entry offered to 0x4777f0 in table order; Cote d'Azur lighthouse (BOX01,
  OBJECT01) at every step 0..6000; the other players without `tr` every 13th step to 6000,
  those with `tr` (taken by an earlier bundle in the game) every 997th.

Rows (all floats as f32 hex words):
  key    case article key 13 words (quaternion x y z w, position, f32, scale xyz, f32, f32)
  claim  case offered claimed [edi+0x18] article indexes in player order
  player case slot article flags keytime count start state   (record +0 +4 +8, player +8 +0xc)
  pose   case slot frame sched state 16 words of the 4x4 written by slot 5

    python scripts/research/replay-lighthouse-anim.py \
        --out iterations/012-campaign-fidelity/crates/nfs-assets/tests/fixtures/scenery-anim.tsv
    python scripts/research/replay-lighthouse-anim.py --tracks --game-dir local/game \
        --out local/fixtures/scenery-anim-tracks.tsv
"""
import argparse
import math
from pathlib import Path
import random
import struct
import sys

from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ai_harness import AiMachine, refpack  # noqa: E402
from vehicle_harness import ROOT, load_module  # noqa: E402

BUNDLE_CTOR = 0x477710
BUNDLE_CLAIM = 0x4777f0
BUNDLE_POSE = 0x4777b0       # slot 5 (index, matrix*)
LIST_INIT = 0x471700
ANIMDEFS_FIND = 0x47edf0     # thiscall, 1 arg
RENDER_OBJ = 0x44ff60        # thiscall, 1 arg
G_ANIM_LIST = 0x628938
G_SCHED_STEP = 0x606ac4
CRP_BASE = 0x10000000
CRP_SIZE = 0x1000000
MATRIX = 0x2008000           # vehicle_harness SCRATCH
SENTINEL = 0x7fc0dead

ANQT, BASE, NAME, TR = 0x416e7174, 0x42617365, 0x4e616d65, 0x74720000
TRACKS = ('alps', 'autobahn', 'castle', 'coastal', 'farmland', 'industrial')
LIGHTHOUSE = ('BOX01', 'OBJECT01')


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def words(raw):
    return ','.join(f'{w:08x}' for w in struct.unpack(f'<{len(raw) // 4}I', raw))


class SceneryMachine(AiMachine):
    def __init__(self, module, data):
        super().__init__(module, data)
        self.uc.mem_map(CRP_BASE, CRP_SIZE)
        # thiscall stubs (callee pops `n` dword args): {va: (handler, n)}
        self.this_stubs = {ANIMDEFS_FIND: (lambda m: 0, 1),
                           RENDER_OBJ: (lambda m: m.alloc(0x14), 1)}

    def _guard(self, uc, address, size, user):
        if address in self.this_stubs:
            fn, n = self.this_stubs[address]
            eax = fn(self)
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret = struct.unpack('<I', uc.mem_read(sp, 4))[0]
            uc.reg_write(UC_X86_REG_ESP, sp + 4 + 4 * n)
            uc.reg_write(UC_X86_REG_EAX, eax & 0xffffffff)
            uc.reg_write(UC_X86_REG_EIP, ret)
            return
        super()._guard(uc, address, size, user)

    def thiscall(self, entry, this, *args, budget=20_000_000):
        return self.call(entry, *args, regs={'ecx': this}, budget=budget)


# -- CRP images ---------------------------------------------------------------
def build_crp(articles):
    """Synthetic CRP image: [(children)] with children = [(ident, count, data bytes)].
    Article entries {'Arti', 0x1a, n, rel16}; children sorted by ident, {ident, flags 0xea |
    size << 8, count, rel bytes}. Returns (image, [article entry offset or None])."""
    img = bytearray(16 * (len(articles) + 1))
    offsets = []
    for i, children in enumerate(articles):
        if children is None:
            offsets.append(None)
            continue
        o = 16 + 16 * i
        children = sorted(children, key=lambda c: c[0])
        table = len(img)
        img += bytes(16 * len(children))
        struct.pack_into('<4I', img, o, 0x41727469, 0x1a, len(children), (table - o) // 16)
        for k, (ident, count, data) in enumerate(children):
            while len(img) % 16:
                img.append(0)
            at = len(img)
            img += data
            co = table + 16 * k
            struct.pack_into('<4I', img, co, ident, 0xea | len(data) << 8, count, at - co)
        while len(img) % 16:
            img.append(0)
        offsets.append(o)
    return bytes(img), offsets


def track_articles(crp):
    """Article entry offsets of a track CRP, [(index, offset, name, anqt?, tr?)]."""
    ac = struct.unpack_from('<I', crp, 4)[0] >> 5
    start = struct.unpack_from('<I', crp, 12)[0] * 16
    out = []
    for i in range(ac):
        o = start + 16 * i
        ident, lf, count, off = struct.unpack_from('<4I', crp, o)
        assert ident == 0x41727469 and lf & 0x0a == 0x0a, (i, hex(lf))
        table = o + off * 16
        idents, name = [], f'article{i}'
        for k in range((lf >> 5) + count):
            ci, clf, _cc, coff = struct.unpack_from('<4I', crp, table + 16 * k)
            idents.append(ci)
            if ci == NAME:
                d = table + 16 * k + coff
                name = crp[d:d + 64].split(b'\0')[0].decode('latin1')
        out.append((i, o, name, ANQT in idents, TR in idents))
    return out


def anqt_keys(m, article):
    """(count, key bytes) of an article's Anqt entry as the bundle reads it."""
    lf, count, off = struct.unpack('<3I', m.get_bytes(article + 4, 12))
    table = article + off * 16
    for k in range((lf >> 5) + count):
        ci, clf, cc, coff = struct.unpack('<4I', m.get_bytes(table + 16 * k, 16))
        if ci == ANQT:
            data = table + 16 * k + coff if clf & 2 else coff
            return cc, m.get_bytes(data, 0x34 * cc)
    raise KeyError('no Anqt')


# -- recording ----------------------------------------------------------------
class Recorder:
    def __init__(self, m):
        self.m = m
        self.rows = []

    def setup(self, image, start_step):
        m = self.m
        m.heap = 0x3000000
        m.uc.mem_write(CRP_BASE, bytes(CRP_SIZE))
        m.put_bytes(CRP_BASE, image)
        m.put_u32(G_ANIM_LIST, 0)
        m.put_u32(G_SCHED_STEP, start_step)
        m.call(LIST_INIT, G_SCHED_STEP, bits(1 / 64))

    def claim(self, case, offered, labels):
        """Runs ctor + 0x4777f0 on `offered` (article addresses, 0 = empty slot); returns
        [(slot, label, player, start)] in player order."""
        m = self.m
        bundle = m.alloc(0x38)
        m.thiscall(BUNDLE_CTOR, bundle, 0)
        arr = m.alloc(4 * len(offered))
        m.put_bytes(arr, b''.join(struct.pack('<I', a) for a in offered))
        claimed = m.thiscall(BUNDLE_CLAIM, bundle, arr, len(offered))
        assert claimed == m.get_u32(bundle + 0x18)
        entries = m.get_u32(bundle + 0x1c)
        players = m.get_u32(bundle + 0x34)
        by_addr = dict(zip(offered, labels))
        out = []
        for slot in range(claimed):
            article = m.get_u32(entries + 8 * slot + 4)
            player = m.get_u32(players + 16 * slot)
            rec = m.get_u32(player + 0x10)
            flags, keytime, count, keys = struct.unpack('<4I', m.get_bytes(rec, 16))
            start, state = m.get_u32(player + 8), m.get_u32(player + 0xc)
            label = by_addr[article]
            n, raw = anqt_keys(m, article)
            assert count == n and raw == m.get_bytes(keys, 0x34 * n), label
            self.rows.append(['player', case, slot, label, f'{flags:08x}', f'{keytime:08x}', count,
                              start, state])
            out.append((slot, label, raw))
        # Offered slots are zeroed for the bundles after this one.
        left = [m.get_u32(arr + 4 * i) for i in range(len(offered))]
        self.rows.append(['claim', case, len([a for a in offered if a]), claimed,
                          ','.join(str(by_addr[a]) for a in offered if a and a not in left)])
        return bundle, out

    def keys(self, case, label, raw, done):
        if (case, label) in done:
            return
        done.add((case, label))
        for k in range(len(raw) // 0x34):
            self.rows.append(['key', case, label, k, words(raw[0x34 * k:0x34 * k + 0x34])])

    def pose(self, case, bundle, slot, frame, sched):
        m = self.m
        m.put_u32(G_SCHED_STEP, sched)
        m.put_bytes(MATRIX, struct.pack('<16I', *[SENTINEL] * 16))
        m.thiscall(BUNDLE_POSE, bundle, slot, MATRIX)
        player = m.get_u32(m.get_u32(bundle + 0x34) + 16 * slot)
        state = m.get_u32(player + 0xc)
        self.rows.append(['pose', case, slot, frame, sched, state, words(m.get_bytes(MATRIX, 64))])


def synthetic_articles():
    rnd = random.Random(5)

    def keys(n):
        out = b''
        for k in range(n):
            q = [rnd.uniform(-1, 1) for _ in range(4)]
            p = [rnd.uniform(-500, 500), rnd.uniform(-20, 80), rnd.uniform(-500, 500)]
            s = [rnd.choice([1.0, rnd.uniform(0.5, 1.5)]) for _ in range(3)]
            out += struct.pack('<13f', *q, *p, 1.0, *s, 1.0, 0.0)
        return out

    base = (BASE, 0, bytes(0x70))
    tr = (TR, 1, struct.pack('<16f', *[1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 5, 6, 7, 1]))
    return [
        [(ANQT, 7, keys(7)), base, (NAME, 0, b'SYN_A\0\0\0')],
        [(ANQT, 5, keys(5)), base, tr, (NAME, 0, b'SYN_TR\0\0')],
        [base, (NAME, 0, b'SYN_STATIC\0\0')],
        [(ANQT, 3, keys(3)), base, (NAME, 0, b'SYN_MIN\0')],
        None,
    ]


def record_synthetic(module, data):
    m = SceneryMachine(module, data)
    r = Recorder(m)
    image, offsets = build_crp(synthetic_articles())
    done = set()
    for start in (0, 37):
        case = f'syn_start{start}'
        r.setup(image, start)
        offered = [CRP_BASE + o if o is not None else 0 for o in offsets]
        labels = [i if o is not None else -1 for i, o in enumerate(offsets)]
        bundle, players = r.claim(case, offered, labels)
        steps = sorted(set(list(range(0, 120)) + list(range(120, 3000, 23))
                           + [s for w in range(1, 40) for s in range(int(w * 6.4 * 5) - 3, int(w * 6.4 * 5) + 4)]
                           + [2**20, 2**31 - 1]))
        for slot, label, raw in players:
            r.keys(case, label, raw, done)
            for i, s in enumerate(steps):
                r.pose(case, bundle, slot, i, s)
    # Frame clocks: g_SchedStep = floor(frame * 64 / fps), one or two pose calls per frame.
    for fps, calls in ((60, 1), (20, 2)):
        case = f'syn_fps{fps}'
        r.setup(image, 0)
        offered = [CRP_BASE + o if o is not None else 0 for o in offsets]
        labels = [i if o is not None else -1 for i, o in enumerate(offsets)]
        bundle, players = r.claim(case, offered, labels)
        for slot, label, raw in players:
            r.keys(case, label, raw, done)
        for frame in range(fps * 6):
            sched = frame * 64 // fps
            for slot, _label, _raw in players:
                for _ in range(calls):
                    r.pose(case, bundle, slot, frame, sched)
    return r.rows


def record_tracks(module, data, game_dir):
    m = SceneryMachine(module, data)
    r = Recorder(m)
    done = set()
    for track in TRACKS:
        crp = (game_dir / f'GameData/Track/{track}.crp').read_bytes()
        if crp[1] == 0xfb and crp[0] & 0xfe == 0x10:
            crp = refpack(crp)
        assert len(crp) <= CRP_SIZE
        arts = track_articles(crp)
        anim = [a for a in arts if a[3]]
        r.setup(crp, 0)
        case = track
        offered = [CRP_BASE + a[1] for a in anim]
        bundle, players = r.claim(case, offered, [a[0] for a in anim])
        names = {a[0]: a for a in arts}
        for slot, label, raw in players:
            name, has_tr = names[label][2], names[label][4]
            r.rows.append(['article', case, label, name, int(has_tr)])
            light = track == 'coastal' and name in LIGHTHOUSE
            r.keys(case, label, raw, done)
            if has_tr and not light:
                # Articles with `tr` are taken by an earlier bundle in the game (read, 0x4879d0);
                # a few steps only, to show the player treats them alike.
                steps = range(0, 6001, 997)
            else:
                steps = range(0, 6001) if light else range(0, 6001, 13)
            for s in steps:
                r.pose(case, bundle, slot, s, s)
    return r.rows


def write(path, rows, comments):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'w') as f:
        for c in comments:
            f.write(f'# {c}\n')
        for row in rows:
            f.write('\t'.join(str(v) for v in row) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--module', default='nfs5.exe')
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--tracks', action='store_true', help='record the game CRPs (local fixture)')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    module, data = load_module(args.module, args.game_dir)
    rec = (lambda: record_tracks(module, data, args.game_dir)) if args.tracks else \
        (lambda: record_synthetic(module, data))
    runs = [rec() for _ in range(2)]
    if runs[0] != runs[1]:
        raise RuntimeError('non-deterministic recording')
    write(args.out, runs[0], [
        'generated by scripts/research/replay-lighthouse-anim.py (deterministic across two runs)',
        'WAnimBundle 0x477710/0x4777f0 claim + CamAnim 0x471490/0x4716e0, per frame slot 5 0x4777b0 -> '
        '0x4715a0 -> 0x470e90 -> 0x470f70; g_SchedStep written by the recorder; FPU CW 0x027f',
        'rows: key case article k words13 | claim case offered claimed taken | '
        'player case slot article flags keytime count start state | '
        'article case index name has_tr | pose case slot frame sched state words16',
    ])
    print(f'{args.out.name}: {len(runs[0])} rows')


if __name__ == '__main__':
    main()
