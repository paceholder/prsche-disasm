"""Unicorn harness for the original render-state code: nfs5.exe plus the
THRASH render driver dx7z.dll (Drivers/dx7, 2018 build, the one in
research/binary-index/static/binaries.jsonl), executed together.

- Both PE images are mapped at their preferred bases (nfs5.exe 0x400000,
  dx7z.dll 0x60000000; no relocation needed). Execution is guarded to the two
  .text sections; the driver's import slots (IAT) point at stubs that stop the
  run with the import's name, so any path that leaves the recorded code shows.
- nfs5.exe reaches the driver only through the THRASH function pointers it
  fills at driver load: [0x6bd97c] = `THRASH_setstate@8`, [0x6bd984] =
  `THRASH_getstate@4` (both stdcall). The harness writes the driver's export
  addresses there.
- The driver talks to Direct3D 7 through the device pointer [0x60058d9c]
  (IDirect3DDevice7). The harness puts a fake device there whose vtable
  entries are stubs: every COM call is logged (method, arguments) and returns
  D3D_OK. SetRenderState (vtable +0x50) and SetTextureStageState (+0x94) are
  the recorded output.

The driver's .data/.bss starts as in the file (device caps zero, i.e. the
state before THRASH_init enumerates the device); recorders set the caps words
they vary explicitly.

Library use: `from render_harness import ...` with scripts/research on
sys.path (the recorders do this themselves).
"""
import hashlib
import json
from pathlib import Path
import struct

from unicorn import Uc, UcError, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE, UC_HOOK_MEM_INVALID
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP, UC_X86_REG_FPCW

ROOT = Path(__file__).resolve().parents[2]

DATA, DATA_SIZE = 0x2000000, 0x40000      # synthetic objects
STUBS, STUBS_SIZE = 0x2100000, 0x10000    # stub entry points (16 bytes apart)
STACK_TOP, EXIT = 0x2210000, 0x2300000

THRASH_SETSTATE_PTR, THRASH_GETSTATE_PTR = 0x6bd97c, 0x6bd984
DRIVER_DEVICE = 0x60058d9c

# IDirect3DDevice7 vtable: (index, name, stack dwords including `this`).
D3D7_METHODS = [
    (0, 'QueryInterface', 3), (1, 'AddRef', 1), (2, 'Release', 1), (3, 'GetCaps', 2),
    (4, 'EnumTextureFormats', 3), (5, 'BeginScene', 1), (6, 'EndScene', 1), (7, 'GetDirect3D', 2),
    (8, 'SetRenderTarget', 3), (9, 'GetRenderTarget', 2), (10, 'Clear', 7), (11, 'SetTransform', 3),
    (12, 'GetTransform', 3), (13, 'SetViewport', 2), (14, 'MultiplyTransform', 3),
    (15, 'GetViewport', 2), (16, 'SetMaterial', 2), (17, 'GetMaterial', 2), (18, 'SetLight', 3),
    (19, 'GetLight', 3), (20, 'SetRenderState', 3), (21, 'GetRenderState', 3),
    (22, 'BeginStateBlock', 1), (23, 'EndStateBlock', 2), (24, 'PreLoad', 2),
    (25, 'DrawPrimitive', 6), (26, 'DrawIndexedPrimitive', 8), (27, 'SetClipStatus', 2),
    (28, 'GetClipStatus', 2), (29, 'DrawPrimitiveStrided', 6), (30, 'DrawIndexedPrimitiveStrided', 8),
    (31, 'DrawPrimitiveVB', 6), (32, 'DrawIndexedPrimitiveVB', 8), (33, 'ComputeSphereVisibility', 6),
    (34, 'GetTexture', 3), (35, 'SetTexture', 3), (36, 'GetTextureStageState', 4),
    (37, 'SetTextureStageState', 4), (38, 'ValidateDevice', 2), (39, 'ApplyStateBlock', 2),
    (40, 'CaptureStateBlock', 2), (41, 'DeleteStateBlock', 2), (42, 'CreateStateBlock', 3),
    (43, 'Load', 6), (44, 'LightEnable', 3), (45, 'GetLightEnable', 3), (46, 'SetClipPlane', 3),
    (47, 'GetClipPlane', 3), (48, 'GetInfo', 4),
]


def load_module(name, game_dir=None):
    modules = [json.loads(line) for line in
               (ROOT / 'research/binary-index/static/binaries.jsonl').read_text().splitlines()]
    module = next(m for m in modules if m['file'] == name)
    data = ((game_dir or ROOT / 'local/game') / module['path']).read_bytes()
    if hashlib.sha256(data).hexdigest() != module['sha256']:
        raise RuntimeError(f'{name}: SHA256 mismatch')
    return module, data


def pe_imports(data, base):
    """[(iat slot VA, 'dll!name')] from the PE import directory."""
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    nsec = struct.unpack_from('<H', data, pe + 6)[0]
    opt = struct.unpack_from('<H', data, pe + 20)[0]
    secs = [struct.unpack_from('<IIII', data, pe + 24 + opt + 40 * i + 8) for i in range(nsec)]

    def off(rva):
        for vsize, va, rsize, raw in secs:
            if va <= rva < va + max(vsize, rsize):
                return raw + rva - va
        raise ValueError(hex(rva))

    imp_rva = struct.unpack_from('<I', data, pe + 24 + 104)[0]
    out = []
    d = off(imp_rva)
    while True:
        ilt, _, _, name_rva, iat = struct.unpack_from('<IIIII', data, d)
        if not name_rva:
            break
        dll = data[off(name_rva):].split(b'\0')[0].decode()
        i = 0
        while True:
            entry = struct.unpack_from('<I', data, off(ilt or iat) + 4 * i)[0]
            if not entry:
                break
            fn = f'#{entry & 0xffff}' if entry & 0x80000000 else \
                data[off(entry) + 2:].split(b'\0')[0].decode()
            out.append((base + iat + 4 * i, f'{dll}!{fn}'))
            i += 1
        d += 20
    return out


class RenderMachine:
    def __init__(self, game_dir=None, fpcw=0x027f):
        self.fpcw = fpcw
        self.uc = uc = Uc(UC_ARCH_X86, UC_MODE_32)
        self.code = []
        self.exports = {}
        for name in ('nfs5.exe', 'dx7z.dll'):
            module, data = load_module(name, game_dir)
            base = int(module['image_base'], 16)
            secs = module['sections']
            end = max(s['rva'] + max(s['vsize'], s['raw_size']) for s in secs)
            uc.mem_map(base, (end + 0xfff) & ~0xfff)
            uc.mem_write(base, data[:0x1000])
            for s in secs:
                uc.mem_write(base + s['rva'], data[s['raw_offset']:s['raw_offset'] + s['raw_size']])
            text = next(s for s in secs if s['name'] == '.text')
            self.code.append((base + text['rva'], base + text['rva'] + text['vsize']))
            for e in module.get('exports') or []:
                if isinstance(e, dict) and 'name' in e and 'rva' in e:
                    rva = e['rva'] if isinstance(e['rva'], int) else int(e['rva'], 16)
                    self.exports[e['name']] = base + rva
            if name == 'dx7z.dll':
                self.driver_imports = pe_imports(data, base)
        for addr, size in ((DATA, DATA_SIZE), (STUBS, STUBS_SIZE),
                           (STACK_TOP - 0x10000, 0x10000), (EXIT, 0x1000)):
            uc.mem_map(addr, size)
        self.stubs = {}   # va -> (name, stack dwords to pop after the return address, handler)
        self.next_stub = STUBS
        for slot, name in self.driver_imports:
            self.put_u32(slot, self.add_stub(name, 0, None))
        self.log = []
        self.device = self._fake_device()
        uc.hook_add(UC_HOOK_CODE, self._guard)
        uc.hook_add(UC_HOOK_MEM_INVALID, self._invalid)

    # -- stubs ---------------------------------------------------------------
    def add_stub(self, name, pop, handler):
        va = self.next_stub
        self.next_stub += 16
        self.stubs[va] = (name, pop, handler)
        return va

    def _fake_device(self):
        """IDirect3DDevice7 at DATA+0x30000 with a stub vtable at +0x100."""
        dev, vtbl = DATA + 0x30000, DATA + 0x30100
        self.put_u32(dev, vtbl)
        for index, name, dwords in D3D7_METHODS:
            def handler(m, args, name=name):
                m.log.append((name, tuple(args[1:])))
                return 0
            self.put_u32(vtbl + 4 * index, self.add_stub(name, dwords, handler))
        return dev

    def _guard(self, uc, address, size, _user):
        if address in self.stubs:
            name, pop, handler = self.stubs[address]
            if handler is None:
                raise RuntimeError(f'import called: {name} (stub needed)')
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret = struct.unpack('<I', uc.mem_read(sp, 4))[0]
            args = list(struct.unpack(f'<{pop}I', uc.mem_read(sp + 4, 4 * pop))) if pop else []
            eax = handler(self, args)
            uc.reg_write(UC_X86_REG_ESP, sp + 4 + 4 * pop)
            uc.reg_write(UC_X86_REG_EAX, (eax or 0) & 0xffffffff)
            uc.reg_write(UC_X86_REG_EIP, ret)
            return
        if address != EXIT and not any(lo <= address < hi for lo, hi in self.code):
            raise RuntimeError(f'execution left .text: {address:08x}')

    @staticmethod
    def _invalid(_uc, access, address, size, _value, _user):
        raise RuntimeError(f'invalid memory access type {access} at {address:08x} size {size}')

    # -- memory --------------------------------------------------------------
    def put_u32(self, addr, value):
        self.uc.mem_write(addr, struct.pack('<I', value & 0xffffffff))

    def put_i32(self, addr, value):
        self.uc.mem_write(addr, struct.pack('<i', value))

    def get_u32(self, addr):
        return struct.unpack('<I', self.uc.mem_read(addr, 4))[0]

    def get_i32(self, addr):
        return struct.unpack('<i', self.uc.mem_read(addr, 4))[0]

    def get_f32(self, addr):
        return struct.unpack('<f', self.uc.mem_read(addr, 4))[0]

    def put_bytes(self, addr, data):
        self.uc.mem_write(addr, bytes(data))

    def get_bytes(self, addr, size):
        return bytes(self.uc.mem_read(addr, size))

    # -- calls ---------------------------------------------------------------
    def call(self, entry, *args, ecx=None, budget=1_000_000):
        """Push dword args (right to left) and a return to EXIT; returns EAX.
        Works for cdecl, stdcall and (with `ecx`) thiscall."""
        sp = STACK_TOP - 0x1000
        frame = struct.pack('<I', EXIT) + b''.join(struct.pack('<I', a & 0xffffffff) for a in args)
        self.uc.mem_write(sp, frame)
        self.uc.reg_write(UC_X86_REG_ESP, sp)
        self.uc.reg_write(UC_X86_REG_FPCW, self.fpcw)
        if ecx is not None:
            self.uc.reg_write(UC_X86_REG_ECX, ecx)
        try:
            self.uc.emu_start(entry, EXIT, count=budget)
        except UcError as error:
            raise RuntimeError(f'emulation failed at {self.uc.reg_read(UC_X86_REG_EIP):08x}: {error}')
        if self.uc.reg_read(UC_X86_REG_EIP) != EXIT:
            raise RuntimeError('instruction budget exhausted')
        return self.uc.reg_read(UC_X86_REG_EAX)
