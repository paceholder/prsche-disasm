"""Record the original race-intro camera (AnimCam, id 14) by executing nfs5.exe x86 code.

Runs `Camera_UpdateAnim 0x462f30` (the AnimCam case of Camera_UpdateAll 0x463c30) in
Unicorn for player 0, step by step, on the real `CmAn` camera animations of a track CRP
and a synthetic car pose, and writes `camera-anim.tsv` (inputs + original outputs) for
crates/nfs-assets/tests/original_camera_anim.rs. No original instruction is patched.
Notes: docs/re/race-intro.md in the workspace root.

    python3 scripts/research/replay-camera-anim.py --game-dir local/game \
        --out-dir local/fixtures

What runs original code: Camera_UpdateAnim 0x462f30 itself, the player factory
CamAnimList_NewPlayer 0x471a50 (thiscall on [0x628938], picks animation
(index + 1) % count), the player's vtable 0x5b4634 slot 1 0x471670 -> CamAnim_Sample
0x470e90 -> CamAnim_Pose 0x470f70 (key lerp 0x48a150, quaternion matrix 0x48a1e0,
scale 0x5329a0, 4x4 build 0x539cd0, rotations 0x539bd0, 4x4 multiply 0x559230), the
car-frame transform 0x532a40 (PACODE x87 kernel), 0x532300, 0x532470, and the
orthonormalisation 0x4a4810. The list object comes from the real CamAnimList_Init
0x471700(&g_SchedStep, 1/64).

What is built by hand: the list items, exactly as CamAnimList_LoadTrack 0x471850 makes
them from the track CRP's `CmAn` entries ({1, chunk+0x24, chunk+0x28, chunk+0x2c, 0, 0,
0}; the CRP resource lookups 0x59f690/0x59f700 are not run), and the heap (bump
allocator stubs for 0x59ef90/0x4211d0, free stubs).

Stubs (declared): Camera_Set 0x465790(player, id) records the id it is called with, and
Camera_UpdateAll 0x463c30 (called once when the animation ends) is a no-op, so the row
of the end step shows the AnimCam record as it was left plus the id requested.

Calling conventions (raw bytes): 0x462f30 cdecl (player), `ret`; 0x471a50 thiscall,
`ret 4`; 0x471700 cdecl (clock*, f32 step).
"""
import argparse
import math
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ai_harness import AiMachine, refpack  # noqa: E402
from vehicle_harness import CAR, ROOT, SCRATCH, f32, load_module, write_tsv  # noqa: E402

UPDATE_ANIM = 0x462f30
LIST_INIT = 0x471700
CAMERA_SET = 0x465790
CAMERA_UPDATE_ALL = 0x463c30
G_ANIM_LIST = 0x628938      # list object {0, clock*, f32 step, vector*}
G_ANIM_INDEX = 0x62893c     # last animation index (static, never reset)
G_SCHED_STEP = 0x606ac4
G_REPLAY = 0x5e99f4
REC = 0x627e80
REC_SIZE = 0x10c
ANIM_DATA = SCRATCH         # CmAn payloads are copied here (heap is used for objects)

GLOBALS = {G_REPLAY: 0, 0x6573e8: 0, 0x606ad4: 0}


def cman_entries(crp):
    """[(name, chunk bytes)] of the CmAn table entries in table order."""
    ac = struct.unpack_from('<I', crp, 4)[0] >> 5
    mc, start = struct.unpack_from('<II', crp, 8)
    start *= 16
    out = []
    for i in range(ac + mc):
        o = start + 16 * i
        ident, lf, _count, rel = struct.unpack_from('<IIII', crp, o)
        if struct.pack('>I', ident) != b'CmAn':
            continue
        size = lf >> 8
        target = o + (rel * 16 if size == 0 else rel)
        chunk = crp[target:target + size]
        out.append((chunk[:0x24].split(b'\0')[0].decode('ascii'), chunk))
    return out


def track_crp(path):
    data = Path(path).read_bytes()
    if data[1] == 0xfb and data[0] & 0xfe == 0x10:
        data = refpack(data)
    if data[:4] != b'karT':
        raise ValueError(f'{path}: not a track CRP')
    return data


class AnimMachine(AiMachine):
    def __init__(self, module, data):
        super().__init__(module, data, stubs={
            CAMERA_SET: self._camera_set,
            CAMERA_UPDATE_ALL: lambda m: 0,
        })
        self.set_ids = []

    def _camera_set(self, m):
        self.set_ids.append((m.arg(0), m.arg(1)))
        return 0

    def setup(self, entries):
        """Fresh list with the track's animations; camera record of player 0 = AnimCam."""
        self.heap = 0x3000000
        self.reset_car()
        for addr, value in GLOBALS.items():
            self.put_u32(addr, value)
        self.put_u32(G_ANIM_LIST, 0)
        self.put_u32(G_SCHED_STEP, 0)
        self.call(LIST_INIT, G_SCHED_STEP, struct.unpack('<I', struct.pack('<f', 1 / 64))[0])
        lst = self.get_u32(G_ANIM_LIST)
        vec = self.get_u32(lst + 0xc)
        items = []
        addr = ANIM_DATA
        for _name, chunk in entries:
            self.put_bytes(addr, chunk)
            item = self.alloc(0x20)
            self.put_bytes(item, struct.pack('<I', 1) + chunk[0x24:0x2c]
                           + struct.pack('<IIII', addr + 0x2c, 0, 0, 0))
            items.append(item)
            addr = (addr + len(chunk) + 15) & ~15
        arr = self.alloc(4 * len(items))
        self.put_bytes(arr, b''.join(struct.pack('<I', i) for i in items))
        self.put_u32(vec, arr)
        self.put_u32(vec + 4, arr + 4 * len(items))
        self.put_u32(vec + 8, arr + 4 * len(items))
        self.uc.mem_write(REC, bytes(REC_SIZE))
        self.put_u32(REC + 0x00, CAR)
        self.put_u32(REC + 0x04, CAR)
        self.put_bytes(REC + 0xbc, struct.pack('<h', 14))
        self.put_bytes(REC + 0xc0, bytes([0x20]))
        # Camera list of player 0 (0x658294): what Camera_Set is asked for at the end.
        self.put_u32(0x658294, 2)


def axes(yaw, pitch=0.0, roll=0.0):
    """Rows R, U, F (f32): roll about F, then pitch about R, then yaw about world y."""
    r, u, f = [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]
    c, s = math.cos(roll), math.sin(roll)
    r, u = [c * a + s * b for a, b in zip(r, u)], [-s * a + c * b for a, b in zip(r, u)]
    c, s = math.cos(pitch), math.sin(pitch)
    u, f = [c * a - s * b for a, b in zip(u, f)], [s * a + c * b for a, b in zip(u, f)]
    c, s = math.cos(yaw), math.sin(yaw)
    out = []
    for v in (r, u, f):
        out += [c * v[0] + s * v[2], v[1], -s * v[0] + c * v[2]]
    return [f32(x) for x in out]


def vec(prefix, values):
    return [(f'{prefix}_{c}', v) for c, v in zip('xyz', values)]


def mat(prefix, values):
    return [(f'{prefix}{i}', v) for i, v in enumerate(values)]


def record_case(m, track, entries, name, pick, pose, start_step, steps):
    """`pick` = the animation index the factory should choose ([0x62893c] = pick - 1)."""
    m.setup(entries)
    m.put_u32(G_ANIM_INDEX, (pick - 1) % len(entries))
    m.set_ids = []
    rows = []
    for i in range(steps):
        step = start_step + i
        pos, ax = pose(i)
        m.put(CAR + 0x330, *pos)
        m.put(CAR + 0x364, *ax)
        m.put_u32(G_SCHED_STEP, step)
        m.call(UPDATE_ANIM, 0)
        player = m.get_u32(REC + 0x20)
        set_id = m.set_ids[-1][1] if m.set_ids else -1
        row = [('case', name), ('track', track), ('anim', entries[pick][0]),
               ('index', m.get_u32(G_ANIM_INDEX)), ('step', step), ('i', i)]
        row += vec('pos', pos) + mat('axis', ax)
        row += [('playing', int(player != 0)), ('set_id', set_id)]
        row += vec('eye', m.get(REC + 0x24, 3)) + mat('m', m.get(REC + 0x7c, 9))
        rows.append(row)
        if m.set_ids:
            break
    return rows


def record_all(module, data, game_dir):
    m = AnimMachine(module, data)
    rows = []
    for track in ('alps', 'monaco1'):
        entries = cman_entries(track_crp(game_dir / f'GameData/Track/{track}.crp'))
        still = axes(0.7, 0.05, -0.03)
        pos = [f32(12.5), f32(3.25), f32(-40.0)]
        for k in range(len(entries)):
            rows += record_case(m, track, entries, f'{track}_still_{k}', k,
                                lambda i: (pos, still), 0, 400)
        # A car that rolls and turns (the camera follows the car frame every step), with
        # the clock starting at a nonzero step.
        rows += record_case(
            m, track, entries, f'{track}_moving', 2,
            lambda i: ([f32(1.0 + 0.05 * i), f32(0.5 + 0.002 * i), f32(-3.0 - 0.1 * i)],
                       axes(-0.4 + 0.004 * i, 0.03 * math.sin(i / 20), 0.02 * math.cos(i / 15))),
            37, 400)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--module', default='nfs5.exe')
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--out-dir', type=Path, default=ROOT / 'local/fixtures')
    args = parser.parse_args()
    module, data = load_module(args.module, args.game_dir)
    runs = [record_all(module, data, args.game_dir) for _ in range(2)]
    if runs[0] != runs[1]:
        raise RuntimeError('non-deterministic recording')
    rows = runs[0]
    write_tsv(args.out_dir / 'camera-anim.tsv', rows, [
        'generated by scripts/research/replay-camera-anim.py (deterministic across two runs)',
        'Camera_UpdateAnim 0x462f30 (AnimCam, race intro) on the CmAn animations of the track CRP',
        'per step: g_SchedStep, car pos/axes in; playing (player object alive), set_id (Camera_Set id at the end, -1 none), eye +0x24, matrix +0x7c out'])
    print(f'camera-anim.tsv: {len(rows)} rows')


if __name__ == '__main__':
    main()
