"""Record the original player camera updates by executing nfs5.exe x86 code.

Runs Camera_UpdateTail 0x461e80 (TailCam0/1/2), Camera_UpdateHeli 0x4623d0
(HeliCam0/1/2), Camera_LookAt 0x464bf0, Camera_UpdateBumper 0x461d50 and
Camera_UpdateCockpit 0x461a30 (BumperCam2) in Unicorn on a synthetic car and
camera record, step by step, and writes TSV fixtures (inputs + original
outputs) for crates/nfs-assets/tests/original_camera.rs. No original
instruction is patched. Field meanings: docs/re/camera.md in the workspace root.

    python3 scripts/research/replay-camera.py --game-dir local/game \
        --out-dir local/fixtures

Harness conventions (scripts/research/vehicle_harness.py): image mapped from the
binary index (SHA256 checked), x87 control word 0x27f at every call, execution
guarded to the image code. The guard here also admits PACODE (0x5b0000..): the
transform Vec_TransformByMatrix 0x532a40 dispatches through 0x55a930 to the
hand-written x87 kernel 0x5b10b8 there (CPU-feature bytes [0x5deb20] 3DNow! and
[0x5deb1c] SSE are written 0, so the x87 path runs; that kernel switches the
precision to 24 bits for the multiply-adds and restores the control word).

Calling conventions (raw bytes):
- Camera_UpdateTail(player, variant) / Camera_UpdateHeli(player, variant):
  cdecl, `ret` (caller pops). Camera_UpdateBumper(player),
  Camera_UpdateCockpit(player): cdecl.
- Camera_LookAt(player, target*, eye*, matrix*): cdecl; target NULL = car
  at record+0x04; matrix rows right/up/forward, the previous up row is the
  fallback reference.
- TrackNodes_Step 0x4817d0: thiscall on [0x628bc0], (node, delta, -1), `ret 0xc`.

Stub (declared): Camera_ClampUnderCeiling 0x461370(player, pos*, dy*) -> bool
is replaced by a scripted ceiling (its real body queries the track object grid
through 0x473420 and virtual calls on track objects, which the harness has no
data for). Model, also used by the Rust test: a flat ceiling at height `ceil_y`
spanning world z in [ceil_z0, ceil_z1]; q = f32(car.y + dy); if pos.z is in the
span and q > ceil_y - 1 then dy = f32(max(ceil_y - car.y - 1, 1)) and it returns
1, else 0 and dy is unchanged. Cases without a ceiling never change dy.

Track nodes: a synthetic track object at [0x628bc0] with one section
{first 0, last N-1} and N nodes (stride 0x50, y at +8), so TrackNodes_Step is
node + delta inside the section. The node indices TrackNodes_Step returns are
read back from EAX after each call and recorded (n1, n2) with their heights.
"""
import argparse
import math
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from vehicle_harness import (CAR, ROOT, SCRATCH, Machine, f32, load_module,  # noqa: E402
                             write_tsv)
from unicorn import UC_HOOK_CODE  # noqa: E402
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ESP  # noqa: E402

UPDATE_TAIL, UPDATE_HELI, LOOK_AT = 0x461e80, 0x4623d0, 0x464bf0
UPDATE_BUMPER, UPDATE_COCKPIT = 0x461d50, 0x461a30
CLAMP_UNDER_CEILING = 0x461370
# EAX after the two TrackNodes_Step calls: (tail, heli) return addresses.
NODE_STEP_RETURNS = {0x46204b: 'n1', 0x46206f: 'n2', 0x462606: 'n1', 0x46262a: 'n2'}

REC = 0x627e80              # g_Camera, player 0 (0x10c bytes)
REC_SIZE = 0x10c
PARAMS = 0x5cf610           # g_CameraParams, 16 bytes per id
TRACK = SCRATCH             # fake track object ([0x628bc0])
SECTIONS = SCRATCH + 0x100  # one 0x28-byte section
NODES = SCRATCH + 0x200     # node array, stride 0x50
NODE_COUNT = 64

GLOBALS = {
    0x606a98: 0,    # update gate (with 0x606870)
    0x606870: 0,
    0x606ad4: 0,    # update gate
    0x6573e8: 0,    # game mode (3 + [0x657408]: TailCam1/2 keep k 0.1)
    0x657408: 0,
    0x5e99f4: 0,    # replay mode (LookAt replay shake, Heli k 0.01)
    0x5e99a8: 0,    # g_LookForward
    0x5e99ac: 0,    # g_LookRight
    0x5e99b0: 0,    # g_LookBehind
    0x5e99b4: 0,    # g_LookLeft
    0x657430: 0,    # direction flag (node step sign)
    0x628bc0: TRACK,
}
G_LOOK_FORWARD, G_LOOK_RIGHT, G_LOOK_BEHIND, G_LOOK_LEFT = 0x5e99a8, 0x5e99ac, 0x5e99b0, 0x5e99b4
G_DIR_FLAG = 0x657430
CPU_3DNOW, CPU_SSE = 0x5deb20, 0x5deb1c


def vec(prefix, values):
    return [(f'{prefix}_{c}', v) for c, v in zip('xyz', values)]


def mat(prefix, values):
    return [(f'{prefix}{i}', v) for i, v in enumerate(values)]


class CameraMachine(Machine):
    def __init__(self, module, data):
        super().__init__(module, data, stubs={CLAMP_UNDER_CEILING: self._ceiling})
        base = int(module['image_base'], 16)
        pacode = next(s for s in module['sections'] if s['name'] == 'PACODE')
        self.code = (self.code[0], base + pacode['rva'] + pacode['vsize'])
        self.ceiling = None          # (y, z0, z1) or None
        self.ceiling_hits = 0
        self.nodes = {}
        for addr in NODE_STEP_RETURNS:
            self.uc.hook_add(UC_HOOK_CODE, self._node_step, begin=addr, end=addr)
        self.uc.mem_write(CPU_3DNOW, b'\0')
        self.uc.mem_write(CPU_SSE, b'\0')

    def _node_step(self, uc, address, _size, _user):
        self.nodes[NODE_STEP_RETURNS[address]] = uc.reg_read(UC_X86_REG_EAX)

    def _ceiling(self, m):
        sp = m.uc.reg_read(UC_X86_REG_ESP)
        _player, pos, dy = struct.unpack('<III', m.uc.mem_read(sp + 4, 12))
        if self.ceiling is None:
            return 0
        cy, z0, z1 = self.ceiling
        car_y = m.get(m.get_u32(REC) + 0x334)[0]
        d = m.get(dy)[0]
        q = f32(car_y + d)
        z = m.get(pos + 8)[0]
        if z0 <= z <= z1 and q > cy - 1.0:
            m.put(dy, f32(max(cy - car_y - 1.0, 1.0)))
            self.ceiling_hits += 1
            return 1
        return 0

    def setup(self, node_y):
        self.reset_car()
        for addr, value in GLOBALS.items():
            self.put_u32(addr, value)
        self.uc.mem_write(REC, bytes(REC_SIZE))
        self.put_u32(REC + 0x00, CAR)
        self.put_u32(REC + 0x04, CAR)
        # Track object: +0 node count, +0xc node array, +0x18 -> {begin, end} of sections.
        self.put_i32(TRACK, NODE_COUNT)
        self.put_u32(TRACK + 0xc, NODES)
        self.put_u32(TRACK + 0x18, TRACK + 0x40)
        self.put_u32(TRACK + 0x40, SECTIONS)
        self.put_u32(TRACK + 0x44, SECTIONS + 0x28)
        self.put_i32(SECTIONS, 0)
        self.put_i32(SECTIONS + 0x10, NODE_COUNT - 1)
        for i in range(NODE_COUNT):
            self.put(NODES + i * 0x50 + 8, node_y(i))

    def set_car(self, s):
        self.put_bytes(CAR + 0x8, struct.pack('<h', s['node']))
        self.put(CAR + 0x330, *s['pos'])
        self.put(CAR + 0x364, *s['axes'])
        self.put(CAR + 0x3a8, *s['ext'])
        self.put_i32(CAR + 0xd30, s['d30'])
        self.put(CAR + 0xd58, s['vl'])
        self.put(CAR + 0xd60, s['vf'])


# ---------------------------------------------------------------------------
# Synthetic car motion.
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


class Drive:
    """Integrates a scripted car: yaw rate, speeds, pitch/roll per step (64 Hz)."""

    def __init__(self, half_len=2.1, node=10, d30=1):
        self.pos = [0.0, 0.5, 0.0]
        self.yaw = 0.0
        self.node = node
        self.node_f = float(node)
        self.d30 = d30
        self.ext = [f32(0.9), f32(0.65), f32(half_len)]

    def step(self, vf, vl=0.0, yaw_rate=0.0, pitch=0.0, roll=0.0, climb=0.0, node_rate=0.0):
        dt = 1.0 / 64.0
        self.yaw += yaw_rate * dt
        fx, fz = math.sin(self.yaw), math.cos(self.yaw)
        self.pos = [self.pos[0] + (vf * fx + vl * fz) * dt, self.pos[1] + climb * dt,
                    self.pos[2] + (vf * fz - vl * fx) * dt]
        self.node_f += node_rate
        self.node = int(self.node_f)
        return {'pos': [f32(x) for x in self.pos], 'axes': axes(self.yaw, pitch, roll),
                'ext': self.ext, 'node': self.node, 'd30': self.d30,
                'vl': f32(vl), 'vf': f32(vf)}


def flat(_i):
    return 0.0


# ---------------------------------------------------------------------------
# Tail / Heli cases: (name, func, id, steps, node profile, init, per-step script)
def tail_cases():
    cases = []

    def case(name, func, cam_id, n, script, node_y=flat, settle=0, o=None, ceiling=None,
             dir_flag=0, drive=None):
        cases.append(dict(name=name, func=func, id=cam_id, n=n, script=script, node_y=node_y,
                          settle=settle, o=o, ceiling=ceiling, dir_flag=dir_flag,
                          drive=drive or Drive()))

    # Straight: TailCam0 from the camera-change reset offset -4*F, settled.
    case('straight_tail0', 'tail', 2, 120, lambda d, i: (d.step(30.0), 0), o=[0.0, 0.0, -4.0])
    # Turn-in: constant speed, yaw rate ramps in, body rolls.
    case('turn_tail0', 'tail', 2, 160,
         lambda d, i: (d.step(25.0, yaw_rate=min(i, 60) / 60 * 1.2,
                               roll=min(i, 60) / 60 * 0.07), 0),
         o=[0.0, 1.15, -5.4])
    # Slide: lateral speed, speed sweep (k floor 0.02 ... cap), yaw oscillation.
    for cam_id, name in ((3, 'slide_tail1'), (4, 'slide_tail2'), (2, 'slide_tail0')):
        case(name, 'tail', cam_id, 160,
             lambda d, i: (d.step(4.0 + i * 0.5, vl=12.0 * math.sin(i / 25.0),
                                  yaw_rate=1.5 * math.sin(i / 30.0), roll=0.05 * math.sin(i / 20.0),
                                  pitch=0.02 * math.cos(i / 15.0)), 0),
             o=[1.0, 1.4, -5.0])
    # Slope: quadratic node profile, node index advances, d grows through the clamps.
    quad = lambda i: f32(0.02 * i * i)  # noqa: E731
    case('slope_tail1', 'tail', 3, 140,
         lambda d, i: (d.step(20.0, climb=1.0, pitch=0.1, node_rate=0.25), 0),
         node_y=quad, o=[0.0, 1.35, -6.6], drive=Drive(node=4))
    case('slope_tail2', 'tail', 4, 140,
         lambda d, i: (d.step(20.0, climb=1.0, pitch=0.1, node_rate=0.25), 0),
         node_y=quad, o=[0.0, 1.15, -6.9], drive=Drive(node=4))
    # Reverse node direction (step -3 via [0x657430]): negative d for TailCam2.
    case('slope_tail2_reverse', 'tail', 4, 120,
         lambda d, i: (d.step(18.0, climb=-1.0, pitch=-0.08, node_rate=0.2), 0),
         node_y=quad, o=[0.0, 1.15, -6.9], dir_flag=1, drive=Drive(node=20))
    # Camera change settle: reset offset -4*F and 50-step counter.
    case('settle_tail0', 'tail', 2, 90, lambda d, i: (d.step(15.0, yaw_rate=0.3), 0),
         settle=50, o=[0.0, 0.0, -4.0])
    case('settle_tail1', 'tail', 3, 90, lambda d, i: (d.step(35.0), 0),
         settle=50, o=[0.0, 0.0, -4.0])
    # Look-behind held for steps 40..79 (TailCam2 on a slope: s enters the rigid path).
    case('look_behind_tail2', 'tail', 4, 120,
         lambda d, i: (d.step(22.0, yaw_rate=0.4, node_rate=0.2), int(40 <= i < 80)),
         node_y=quad, o=[0.0, 1.15, -6.9], drive=Drive(node=8))
    case('look_behind_tail0', 'tail', 2, 100,
         lambda d, i: (d.step(22.0, yaw_rate=-0.6), int(30 <= i < 60)), o=[0.0, 1.15, -5.4])
    # Ceiling: a bridge ceiling at y 2.6 (2.1 m above the car centre) the road over z in [20, 35].
    case('ceiling_tail0', 'tail', 2, 120, lambda d, i: (d.step(20.0), 0),
         o=[0.0, 1.15, -5.4], ceiling=(f32(2.6), f32(20.0), f32(35.0)))
    case('ceiling_tail1', 'tail', 3, 120, lambda d, i: (d.step(20.0, vl=3.0), 0),
         o=[0.0, 1.35, -6.6], ceiling=(f32(2.6), f32(20.0), f32(35.0)))
    # HeliCam (port check; the doc's variant decoding 5/6/7 -> 0/1/2).
    for cam_id, name in ((5, 'heli0'), (6, 'heli1'), (7, 'heli2')):
        case(name, 'heli', cam_id, 120,
             lambda d, i: (d.step(10.0 + i * 0.4, vl=6.0 * math.sin(i / 20.0),
                                  yaw_rate=0.8 * math.sin(i / 40.0), climb=0.5, node_rate=0.25), 0),
             node_y=quad, o=[0.0, 0.0, -4.0], settle=50, drive=Drive(node=4))
    case('heli2_look_behind', 'heli', 7, 80,
         lambda d, i: (d.step(15.0, node_rate=0.2), int(20 <= i < 50)),
         node_y=quad, o=[0.0, 4.9, -6.7], drive=Drive(node=6),
         ceiling=(f32(6.0), f32(5.0), f32(9.0)))
    return cases


TABLE_FLAG_BYTE = {2: 0x5a, 3: 0x5a, 4: 0x5a, 5: 0x5a, 6: 0x5a, 7: 0x5a}   # 0x2d << 1


def record_tail(m, c):
    m.setup(c['node_y'])
    m.put_u32(G_DIR_FLAG, c['dir_flag'])
    m.ceiling = c['ceiling']
    rec_id = c['id']
    variant = rec_id - (2 if c['func'] == 'tail' else 5)
    entry = UPDATE_TAIL if c['func'] == 'tail' else UPDATE_HELI
    m.put_bytes(REC + 0xbc, struct.pack('<h', rec_id))
    m.put_bytes(REC + 0xc0, bytes([TABLE_FLAG_BYTE[rec_id]]))
    m.put_bytes(REC + 0xc1, struct.pack('<b', c['settle']))
    drive = c['drive']
    first = None
    rows = []
    for i in range(c['n']):
        state, look_behind = c['script'](drive, i)
        m.set_car(state)
        if first is None:
            # Initial offset is given in car axes; eye = pos + o; matrix = car axes.
            r, u, f = state['axes'][0:3], state['axes'][3:6], state['axes'][6:9]
            o = [f32(c['o'][0] * r[k] + c['o'][1] * u[k] + c['o'][2] * f[k]) for k in range(3)]
            m.put(REC + 0x08, *o)
            m.put(REC + 0x24, *[f32(p + q) for p, q in zip(state['pos'], o)])
            m.put(REC + 0x7c, *state['axes'])
            first = True
        m.put_u32(G_LOOK_BEHIND, look_behind)
        o_in, eye_in = m.get(REC + 0x08, 3), m.get(REC + 0x24, 3)
        settle_in = struct.unpack('<b', m.get_bytes(REC + 0xc1, 1))[0]
        m.nodes = {}
        m.ceiling_hits = 0
        m.call(entry, 0, variant)
        hits = m.ceiling_hits
        n1, n2 = m.nodes['n1'], m.nodes['n2']
        m.call(LOOK_AT, 0, 0, REC + 0x24, REC + 0x7c)
        cy, z0, z1 = c['ceiling'] or (0.0, 0.0, 0.0)
        row = [('case', c['name']), ('step', i), ('func', c['func']), ('id', rec_id),
               ('variant', variant), ('flags', TABLE_FLAG_BYTE[rec_id]),
               ('look_behind', look_behind), ('dir_flag', c['dir_flag']), ('d30', state['d30']),
               ('node', state['node']), ('n1', n1), ('y1', c['node_y'](n1)), ('n2', n2),
               ('y2', c['node_y'](n2)), ('ceiling', int(c['ceiling'] is not None)),
               ('ceil_y', cy), ('ceil_z0', z0), ('ceil_z1', z1)]
        row += vec('pos', state['pos']) + mat('axis', state['axes'])
        row += [('half_len', state['ext'][2]), ('vl', state['vl']), ('vf', state['vf'])]
        row += vec('o_in', o_in) + vec('eye_in', eye_in)
        row += [('settle_in', settle_in)]
        row += vec('o', m.get(REC + 0x08, 3)) + vec('aux', m.get(REC + 0x14, 3))
        row += vec('eye', m.get(REC + 0x24, 3)) + mat('m', m.get(REC + 0x7c, 9))
        row += [('settle', struct.unpack('<b', m.get_bytes(REC + 0xc1, 1))[0]),
                ('ceiling_hits', hits)]
        rows.append(row)
    return rows


# ---------------------------------------------------------------------------
# Bumper cases.
def read_tpg_camera(game_dir, model, style):
    """drivercampos/rot[style] from the car .tpg (style key first, then plain, then the
    loader default for pos; rot defaults to 0)."""
    keys = {}
    for line in (game_dir / f'GameData/CarModel/{model}.tpg').read_text(errors='replace').splitlines():
        if '=' in line:
            k, v = line.split('=', 1)
            keys[k.strip().lower()] = v.strip()

    def get(name, default):
        for k in (f'{name}style{style}', name):
            if k in keys:
                return [f32(float(x)) for x in keys[k].split(',')]
        return default
    return get('drivercampos', [f32(-0.36), f32(0.40), f32(-0.25)]), get('drivercamrot', [0.0] * 3)


def look_script(i):
    """(left, right, behind, forward) held per step."""
    if 30 <= i < 90:
        return 1, 0, 0, 0
    if 120 <= i < 150:
        return 0, 1, 0, 0
    if 170 <= i < 210:
        return 0, 0, 1, 0
    if 230 <= i < 260:
        return 0, 1, 0, 1
    return 0, 0, 0, 0


def record_bumper(m, name, cam_id, model, campos, camrot):
    m.setup(flat)
    m.put(PARAMS + 0x10, *campos)          # Camera_RaceInit: row 1 = drivercampos
    m.put(REC + 0x4c, *camrot)             # record +0x4c = drivercamrot
    m.put_bytes(REC + 0xbc, struct.pack('<h', cam_id))
    m.put_bytes(REC + 0xc0, bytes([0x04 if cam_id == 0 else 0x00]))
    entry = UPDATE_BUMPER if cam_id == 0 else UPDATE_COCKPIT
    params = m.get(PARAMS + 0x10 * cam_id, 3)
    drive = Drive()
    rows = []
    for i in range(300):
        state = drive.step(20.0 + 10.0 * math.sin(i / 50.0), vl=2.0 * math.sin(i / 17.0),
                           yaw_rate=0.9 * math.sin(i / 35.0), pitch=0.03 * math.sin(i / 11.0),
                           roll=0.06 * math.sin(i / 23.0), climb=0.3 * math.cos(i / 40.0))
        m.set_car(state)
        if i == 0:
            m.put(REC + 0x08, *[f32(a) for a in state['axes'][6:9]])   # arbitrary start offset
        left, right, behind, fwd = look_script(i)
        for addr, v in ((G_LOOK_LEFT, left), (G_LOOK_RIGHT, right), (G_LOOK_BEHIND, behind),
                        (G_LOOK_FORWARD, fwd)):
            m.put_u32(addr, v)
        o_in = m.get(REC + 0x08, 3)
        angle_in = struct.unpack('<h', m.get_bytes(REC + 0xc6, 2))[0]
        m.call(entry, 0)
        row = [('case', name), ('step', i), ('car', model), ('id', cam_id),
               ('look_left', left), ('look_right', right), ('look_behind', behind),
               ('look_forward', fwd)]
        row += vec('pos', state['pos']) + mat('axis', state['axes'])
        row += vec('cam', params) + vec('rot', camrot)
        row += vec('o_in', o_in) + [('angle_in', angle_in)]
        row += vec('o', m.get(REC + 0x08, 3)) + vec('aux', m.get(REC + 0x14, 3))
        row += vec('eye', m.get(REC + 0x24, 3)) + mat('m', m.get(REC + 0x7c, 9))
        row += [('angle', struct.unpack('<h', m.get_bytes(REC + 0xc6, 2))[0])]
        rows.append(row)
    return rows


def record_all(module, data, game_dir):
    m = CameraMachine(module, data)
    tail = []
    for c in tail_cases():
        tail += record_tail(m, c)
    bumper = []
    for model, style in (('550', 0), ('993', 0)):
        campos, camrot = read_tpg_camera(game_dir, model, style)
        for cam_id, cam in ((0, 'bumper'), (1, 'bumper2')):
            bumper += record_bumper(m, f'{cam}_{model}', cam_id, model, campos, camrot)
    return tail, bumper


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
    tail, bumper = runs[0]
    note = 'generated by scripts/research/replay-camera.py (deterministic across two runs)'
    write_tsv(args.out_dir / 'camera-tail.tsv', tail, [
        note, 'Camera_UpdateTail 0x461e80 / Camera_UpdateHeli 0x4623d0 + Camera_LookAt 0x464bf0',
        'per step: inputs, state before (o_in, eye_in, settle_in; the matrix starts as the car axes), state after'])
    write_tsv(args.out_dir / 'camera-bumper.tsv', bumper, [
        note, 'Camera_UpdateBumper 0x461d50 / Camera_UpdateCockpit 0x461a30 (BumperCam2)',
        'cam = g_CameraParams row (row 1 = .tpg drivercampos), rot = drivercamrot'])
    print(f'camera-tail.tsv: {len(tail)} rows, camera-bumper.tsv: {len(bumper)} rows')


if __name__ == '__main__':
    main()
