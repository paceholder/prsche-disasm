"""Shared Unicorn harness for the original vehicle model recorders (V1..V6).

Executes original nfs5.exe x86 code on a synthetic car, following
docs/re/vehicle.md "Port" / "Common harness setup" in the workspace root:

- the PE image is mapped from research/binary-index/static/binaries.jsonl
  (SHA256 checked); execution is guarded to .text, so any call into an import
  or other non-.text code stops with an error unless a stub is registered;
- car struct (0x1300 bytes) at CAR, carSpecs (0x1c4) at SPECS = car+0x760,
  resource/tuning struct (0x600) at RES = car+0x558, 20 part-record pointers at
  car+0x768.. pointing at zeroed records (kind +0x26 = 0: no parts, no damage);
- the globals the vehicle code reads are written explicitly (set_globals), so
  fixtures don't depend on .data defaults;
- call helpers return to an EXIT address: cdecl (args on the stack),
  thiscall (ECX = this, args on the stack) and enter_mid for starting inside a
  function with a hand-built frame.

Calling conventions checked in the raw bytes:
- Sim_Load 0x49c6c0: cdecl, car at [esp+4]; frame `sub esp,0x64; push ebx;
  push esi; push edi`, esi = car; epilogue at 0x49ca1f (pop edi/esi/ebx,
  add esp,0x64, ret).
- Sim_DeriveFields 0x49cb50: cdecl, car at [esp+4].
- _stricmp 0x5ae3c0 and __ftol 0x5a0f98 are statically linked CRT code inside
  .text (no stub needed); _stricmp reads the locale flag [0x6afd80] (set 0).

x87 control word: every call starts with FPCW = 0x027f (all exceptions masked,
53-bit precision, round to nearest), the state the CRT start-up leaves:
`_fpinit` 0x5a3dba calls `_controlfp(_PC_53, _MCW_PC)` (0x5ab7dc). Unicorn
honours the precision field and starts with FPCW = 0 (24-bit precision), which
the older recorders (replay-car-pair.py) inherit. **Hypothesis:** the render
driver leaves the precision alone during Sim_Load (not checked; a D3D device
without FPU-preserve would switch to 24 bits).

Library use: `from vehicle_harness import ...` with scripts/research on
sys.path (the recorders do this themselves).
"""
import hashlib
import json
from pathlib import Path
import struct

from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_INVALID
from unicorn.x86_const import (UC_X86_REG_EAX, UC_X86_REG_EBP, UC_X86_REG_EBX, UC_X86_REG_ECX,
                               UC_X86_REG_EDI, UC_X86_REG_EDX, UC_X86_REG_EIP, UC_X86_REG_ESI,
                               UC_X86_REG_ESP, UC_X86_REG_FPCW)

ROOT = Path(__file__).resolve().parents[2]

# Synthetic memory layout (outside the image, 0x400000..0x6c2000).
CAR = 0x2000000
CAR_SIZE = 0x1300
SPECS = 0x2002000
SPECS_SIZE = 0x1c4
RES = 0x2003000
RES_SIZE = 0x600
PARTS = 0x2004000          # 20 records of PART_SIZE
PART_SIZE = 0x100          # highest field read by 0x49cb50 is +0xec
PART_COUNT = 20
SCRATCH = 0x2008000        # free area for callers (0x8000 bytes)
DATA_SIZE = 0x10000
STACK_TOP, EXIT = 0x2110000, 0x2200000

CAR_SPECS_PTR = 0x760
CAR_RES_PTR = 0x558
CAR_PARTS_PTR = 0x768

REGS = {'eax': UC_X86_REG_EAX, 'ebx': UC_X86_REG_EBX, 'ecx': UC_X86_REG_ECX,
        'edx': UC_X86_REG_EDX, 'esi': UC_X86_REG_ESI, 'edi': UC_X86_REG_EDI,
        'ebp': UC_X86_REG_EBP}

# Globals read by the vehicle code (docs/re/vehicle.md "Common harness setup").
# Values written by set_globals(); override per recorder.
GLOBALS = {
    'g_SchedStep': (0x606ac4, 0x1c4),        # >= 0x1c4 passes the race-start gate
    'g_RandState': (0x5d1028, 0xf874af01),   # LCG state, shipped .data value
    'g_RandMul': (0x5d102c, 1),              # LCG multiplier, shipped .data value
    'g_HandlingModeFlag': (0x6573f8, 0),
    'g_AutoBox': (0x658098, 1),              # player 0 automatic transmission
    'g_TrackIsAutobahn': (0x606ae0, 0),      # avoids the spatial query 0x473330
    'g_CarCount': (0x5e8ae8, 0),             # draft factor 1
    'g_SplitScreen': (0x657404, 0),
    'g_PairDamp': (0x657408, 0),
    'g_AiHandbrakeTurn': (0x5e67b0, 0),
    'g_CrtMbcsLocale': (0x6afd80, 0),        # _stricmp 0x5ae3c0: plain ASCII path
}


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def load_module(module_name='nfs5.exe', game_dir=None):
    modules = [json.loads(line) for line in
               (ROOT / 'research/binary-index/static/binaries.jsonl').read_text().splitlines()]
    module, = [m for m in modules if m['file'] == module_name]
    data = ((game_dir or ROOT / 'local/game') / module['path']).read_bytes()
    if hashlib.sha256(data).hexdigest() != module['sha256']:
        raise RuntimeError('original SHA256 mismatch')
    return module, data


class Machine:
    """Unicorn machine with the image mapped and the synthetic car allocated."""

    def __init__(self, module, data, stubs=None, trace_calls=False, fpcw=0x027f):
        self.fpcw = fpcw
        self.uc = uc = Uc(UC_ARCH_X86, UC_MODE_32)
        base = int(module['image_base'], 16)
        sections = module['sections']
        end = max(s['rva'] + max(s['vsize'], s['raw_size']) for s in sections)
        uc.mem_map(base, (end + 0xfff) & ~0xfff)
        for s in sections:
            uc.mem_write(base + s['rva'], data[s['raw_offset']:s['raw_offset'] + s['raw_size']])
        text = next(s for s in sections if s['name'] == '.text')
        self.code = (base + text['rva'], base + text['rva'] + text['vsize'])
        for addr, size in ((CAR, DATA_SIZE), (STACK_TOP - 0x10000, 0x10000), (EXIT, 0x1000)):
            uc.mem_map(addr, size)
        # {va: handler(machine) -> eax}; the stub returns like a cdecl function.
        self.stubs = dict(stubs or {})
        self.calls = {}           # call target -> count (trace_calls)
        self.trace_calls = trace_calls
        uc.hook_add(UC_HOOK_CODE, self._guard)
        uc.hook_add(UC_HOOK_MEM_INVALID, self._invalid)

    # -- hooks -------------------------------------------------------------
    def _guard(self, uc, address, size, _user):
        if address in self.stubs:
            eax = self.stubs[address](self)
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret = struct.unpack('<I', uc.mem_read(sp, 4))[0]
            uc.reg_write(UC_X86_REG_ESP, sp + 4)
            uc.reg_write(UC_X86_REG_EAX, (eax or 0) & 0xffffffff)
            uc.reg_write(UC_X86_REG_EIP, ret)
            return
        if not (self.code[0] <= address < self.code[1] or address == EXIT):
            raise RuntimeError(f'execution left .text: {address:08x} (stub needed)')
        if self.trace_calls and size == 5:
            insn = uc.mem_read(address, 5)
            if insn[0] == 0xe8:
                target = (address + 5 + struct.unpack('<i', insn[1:])[0]) & 0xffffffff
                self.calls[target] = self.calls.get(target, 0) + 1

    @staticmethod
    def _invalid(_uc, access, address, size, _value, _user):
        raise RuntimeError(f'invalid memory access type {access} at {address:08x} size {size}')

    # -- memory helpers ----------------------------------------------------
    def put(self, addr, *values):
        self.uc.mem_write(addr, struct.pack('<' + 'f' * len(values), *values))

    def get(self, addr, count=1):
        return list(struct.unpack('<' + 'f' * count, self.uc.mem_read(addr, 4 * count)))

    def put_i32(self, addr, *values):
        self.uc.mem_write(addr, struct.pack('<' + 'i' * len(values), *values))

    def get_i32(self, addr, count=1):
        return list(struct.unpack('<' + 'i' * count, self.uc.mem_read(addr, 4 * count)))

    def put_u32(self, addr, value):
        self.uc.mem_write(addr, struct.pack('<I', value & 0xffffffff))

    def get_u32(self, addr):
        return struct.unpack('<I', self.uc.mem_read(addr, 4))[0]

    def put_bytes(self, addr, data):
        self.uc.mem_write(addr, bytes(data))

    def get_bytes(self, addr, size):
        return bytes(self.uc.mem_read(addr, size))

    def put_str(self, addr, text, size):
        raw = text.encode('ascii')
        if len(raw) >= size:
            raise ValueError(f'string too long for {size} bytes: {text!r}')
        self.uc.mem_write(addr, raw + bytes(size - len(raw)))

    # Field helpers relative to the synthetic structs.
    def car_get(self, off, count=1):
        return self.get(CAR + off, count)

    def car_put(self, off, *values):
        self.put(CAR + off, *values)

    def spec_get(self, off, count=1):
        return self.get(SPECS + off, count)

    def spec_put(self, off, *values):
        self.put(SPECS + off, *values)

    def res_put_i32(self, off, *values):
        self.put_i32(RES + off, *values)

    def wheel(self, i):
        """Base address of wheel record i (car+0x7b8 + i*0xc4)."""
        return CAR + 0x7b8 + i * 0xc4

    # -- setup -------------------------------------------------------------
    def reset_car(self):
        """Zero car, carSpecs, res and parts and wire the pointers."""
        self.uc.mem_write(CAR, bytes(DATA_SIZE))
        self.put_u32(CAR + CAR_SPECS_PTR, SPECS)
        self.put_u32(CAR + CAR_RES_PTR, RES)
        for i in range(PART_COUNT):
            self.put_u32(CAR + CAR_PARTS_PTR + 4 * i, PARTS + i * PART_SIZE)

    def set_globals(self, **overrides):
        unknown = set(overrides) - set(GLOBALS)
        if unknown:
            raise KeyError(f'unknown globals: {sorted(unknown)}')
        for name, (addr, value) in GLOBALS.items():
            self.put_u32(addr, overrides.get(name, value))

    # -- calls -------------------------------------------------------------
    def _run(self, entry, sp, regs, budget):
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        self.uc.reg_write(UC_X86_REG_FPCW, self.fpcw)
        for name, value in (regs or {}).items():
            self.uc.reg_write(REGS[name], value & 0xffffffff)
        try:
            self.uc.emu_start(entry, EXIT, count=budget)
        except UcError as error:
            raise RuntimeError(f'emulation failed at {self.uc.reg_read(UC_X86_REG_EIP):08x}: {error}')
        if self.uc.reg_read(UC_X86_REG_EIP) != EXIT:
            raise RuntimeError('instruction budget exhausted')
        return self.uc.reg_read(UC_X86_REG_EAX)

    def call(self, entry, *args, regs=None, budget=2_000_000):
        """cdecl: dword args pushed right to left, return to EXIT; returns EAX."""
        sp = STACK_TOP - 0x1000
        frame = struct.pack('<I', EXIT) + b''.join(struct.pack('<I', a & 0xffffffff) for a in args)
        self.uc.mem_write(sp, frame)
        return self._run(entry, sp, regs, budget)

    def call_thiscall(self, entry, this, *args, budget=2_000_000):
        """thiscall (MSVC): ECX = this, dword args on the stack (callee pops)."""
        return self.call(entry, *args, regs={'ecx': this}, budget=budget)

    def enter_mid(self, entry, frame, regs, budget=2_000_000):
        """Start inside a function: `frame` is the dword stack image from ESP
        upwards (saved registers, locals, return address EXIT, args)."""
        sp = STACK_TOP - 0x1000
        self.uc.mem_write(sp, b''.join(struct.pack('<I', v & 0xffffffff) for v in frame))
        return self._run(entry, sp, regs, budget)


# ---------------------------------------------------------------------------
# carSpecs layout (docs/re/vehicle.md section 5). (name, offset, count, kind)
# kind 'f' = f32, 'i' = i32. Names keep the offset like original_pair.rs.
CARSPECS_FIELDS = [
    ('name_00', 0x00, 16, 'i'),            # car name, 64 ASCII bytes (dumped as i32 words)
    ('mass_40', 0x40, 1, 'f'),
    ('wheelbase_44', 0x44, 1, 'f'),
    ('flag_48', 0x48, 1, 'i'),
    ('engine_type_4c', 0x4c, 1, 'i'),
    ('gear_count_50', 0x50, 1, 'i'),
    ('shift_time_54', 0x54, 1, 'i'),
    ('drive_efficiency_58', 0x58, 1, 'f'),
    ('front_torque_share_5c', 0x5c, 1, 'f'),
    ('gear_ratio_60', 0x60, 8, 'f'),        # R, N, 1..6; rpm per m/s after derivation
    ('final_drive_80', 0x80, 1, 'f'),
    ('tyre_width_84', 0x84, 1, 'f'),
    ('tyre_aspect_88', 0x88, 1, 'f'),
    ('rim_8c', 0x8c, 1, 'f'),
    ('top_speed_90', 0x90, 1, 'f'),
    ('redline_94', 0x94, 1, 'f'),
    ('engine_braking_98', 0x98, 1, 'f'),
    ('torque_9c', 0x9c, 21, 'f'),
    ('front_weight_f0', 0xf0, 1, 'f'),
    ('tyre_mu_f4', 0xf4, 1, 'f'),
    ('cog_height_f8', 0xf8, 1, 'f'),
    ('steer_lock_fc', 0xfc, 1, 'f'),
    ('toe_100', 0x100, 1, 'f'),
    ('brake_decel_104', 0x104, 1, 'f'),
    ('front_brake_share_108', 0x108, 1, 'f'),
    ('front_downforce_10c', 0x10c, 1, 'f'),
    ('rear_downforce_110', 0x110, 1, 'f'),
    ('pitch_retention_114', 0x114, 1, 'f'),
    ('roll_retention_118', 0x118, 1, 'f'),
    ('suspension_stiffness_11c', 0x11c, 1, 'f'),
    ('ride_height_120', 0x120, 1, 'f'),
    ('suspension_travel_124', 0x124, 1, 'f'),
    ('pitch_gain_128', 0x128, 1, 'f'),
    ('roll_gain_12c', 0x12c, 1, 'f'),
    ('lateral_split_130', 0x130, 1, 'f'),
    ('max_pitch_rate_134', 0x134, 1, 'f'),
    ('max_roll_rate_138', 0x138, 1, 'f'),
    ('field_13c', 0x13c, 1, 'f'),
    ('field_140', 0x140, 1, 'f'),
    ('field_144', 0x144, 1, 'f'),
    ('tyre_circumference_148', 0x148, 1, 'f'),
    ('inv_redline_14c', 0x14c, 1, 'f'),
    ('ratio_per_redline_150', 0x150, 8, 'f'),
    ('inv_ratio_170', 0x170, 8, 'f'),
    ('drive_factor_190', 0x190, 8, 'f'),
    ('drag_1b0', 0x1b0, 1, 'f'),
    ('yaw_arm_1b4', 0x1b4, 1, 'f'),
    ('inv_yaw_arm_1b8', 0x1b8, 1, 'f'),
    ('inv_mu_1bc', 0x1bc, 1, 'f'),
    ('tyre_type_1c0', 0x1c0, 1, 'i'),
]


def _check_layout(fields, size):
    pos = 0
    for name, off, count, _ in fields:
        if off != pos:
            raise AssertionError(f'layout gap/overlap at {name}')
        pos = off + 4 * count
    if pos != size:
        raise AssertionError('layout size mismatch')


_check_layout(CARSPECS_FIELDS, SPECS_SIZE)


def unpack_fields(raw, fields):
    """Bytes -> [(column, value)]; arrays get `_i` suffixes."""
    out = []
    for name, off, count, kind in fields:
        values = struct.unpack_from('<' + kind * count, raw, off)
        if count == 1:
            out.append((name, values[0]))
        else:
            out.extend((f'{name}_{i}', v) for i, v in enumerate(values))
    return out


# ---------------------------------------------------------------------------
# TSV writer (same format as replay-car-pair.py: Python repr of the f32 value
# widened to double, which round-trips exactly).
def fmt(x):
    return repr(float(x)) if isinstance(x, float) else str(x)


def write_tsv(path, rows, comments=()):
    header = [k for k, _ in rows[0]]
    lines = [f'# {c}' for c in comments] + ['\t'.join(header)]
    for row in rows:
        if [k for k, _ in row] != header:
            raise RuntimeError(f'column mismatch in {path}')
        lines.append('\t'.join(fmt(v) for _, v in row))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('\n'.join(lines) + '\n', encoding='utf-8')


# ---------------------------------------------------------------------------
# V1: SIM load tail + Sim_DeriveFields.
SIM_LOAD, SIM_LOAD_TAIL, SIM_DERIVE = 0x49c6c0, 0x49c82f, 0x49cb50
SIM_FILE_SIZE = 0x148

# Resource/tuning fields read by 0x49c82f..0x49d9a9 (res = car+0x558). The
# 16.16 values are fe.txt `car[0].*` defaults; the res offsets come from the
# consumer instructions in 0x49cb50 (fe.txt key -> offset is by meaning:
# ratios/final added to sim 0x60..0x80, downforce scales 0x10c/0x110, etc.).
# Everything not listed here is zero, including res+0xc (drive factor x1.05
# when 0), res+0x17c (cheat string, "SILSPD"/"ROLLEY") and res+0x518 (light
# mode); their writers are not traced (vehicle.md open questions).
RES_MODEL_NAME = 0x2a8      # AI spec / model name ("<name>.ais", 0x49ca30), 64 bytes
RES_CHEAT = 0x17c           # compared with "SILSPD"/"ROLLEY" (strcmp inline)
DEFAULT_TUNING = {
    'final_ratio_f0': 0,           # FINALRATIO
    'gear_ratio_f4': [0] * 8,      # res+0xf4+4i added to sim 0x60+4i (i=1, neutral, skipped)
    'front_downforce_114': 3276800,
    'rear_downforce_118': 3276800,
    'brake_bias_11c': 3276800,
    'ride_height_120': 0,
    'suspension_travel_124': 0,
    'steering_toe_128': 0,
    'suspension_stiffness_134': 65536,
    'front_tyre_pressure_13c': 2293760,
    'rear_tyre_pressure_140': 2293760,
    'flag_c': 0,
    'light_mode_518': 0,
    'cheat_17c': '',
}


def write_tuning(m, model, tuning):
    t = dict(DEFAULT_TUNING, **tuning)
    m.uc.mem_write(RES, bytes(RES_SIZE))
    m.put_i32(RES + 0xc, t['flag_c'])
    m.put_i32(RES + 0xf0, t['final_ratio_f0'])
    m.put_i32(RES + 0xf4, *t['gear_ratio_f4'])
    for key in ('front_downforce_114', 'rear_downforce_118', 'brake_bias_11c', 'ride_height_120',
                'suspension_travel_124', 'steering_toe_128', 'suspension_stiffness_134',
                'front_tyre_pressure_13c', 'rear_tyre_pressure_140', 'light_mode_518'):
        m.put_i32(RES + int(key.rsplit('_', 1)[1], 16), t[key])
    m.put_str(RES + RES_CHEAT, t['cheat_17c'], 0x40)
    m.put_str(RES + RES_MODEL_NAME, model, 0x40)


def derive_specs(m, raw_sim, model, tuning=None):
    """Run Sim_Load from 0x49c82f (after the file read, which is memset 0x1c4 +
    memcpy of the file) and Sim_DeriveFields 0x49cb50 on a fresh car.
    Returns the 0x1c4 carSpecs bytes."""
    if len(raw_sim) != SIM_FILE_SIZE:
        raise ValueError('a .sim file is 328 bytes')
    m.reset_car()
    m.set_globals()
    write_tuning(m, model, tuning or {})
    m.put_bytes(SPECS, bytes(raw_sim) + bytes(SPECS_SIZE - SIM_FILE_SIZE))
    # Sim_Load frame at 0x49c82f: [esp] edi, esi, ebx, 0x64 bytes of locals,
    # return address, car argument. esi = car.
    frame = [0, CAR, 0] + [0] * (0x64 // 4) + [EXIT, CAR]
    m.enter_mid(SIM_LOAD_TAIL, frame, {'esi': CAR, 'edi': 0, 'ebx': 0})
    return m.get_bytes(SPECS, SPECS_SIZE)


def _self_test():
    """1.1 check: one car through 0x49c82f/0x49cb50 inside .text."""
    import argparse
    parser = argparse.ArgumentParser(description=_self_test.__doc__)
    parser.add_argument('--game-dir', type=Path, default=ROOT / 'local/game')
    parser.add_argument('--sim', default='993coupe36.sim')
    parser.add_argument('--model', default='993')
    args = parser.parse_args()
    module, data = load_module('nfs5.exe', args.game_dir)
    m = Machine(module, data, trace_calls=True)
    raw = (args.game_dir / 'GameData/Simulation/CarData' / args.sim).read_bytes()
    specs = derive_specs(m, raw, args.model)
    print(f'x87 control word: {m.uc.reg_read(UC_X86_REG_FPCW):#06x}')
    print('calls:', ', '.join(f'{t:#x} x{n}' for t, n in sorted(m.calls.items())))
    for name, value in unpack_fields(specs, CARSPECS_FIELDS):
        if not name.startswith('name_00'):
            print(f'{name}\t{fmt(value)}')


if __name__ == '__main__':
    _self_test()
