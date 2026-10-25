"""Unicorn harness for the original AI recorders (change original-ai).

Builds on vehicle_harness.Machine (image, synthetic car, x87 control word
0x27f) and adds what the AI code needs, following docs/re/ai.md in the
workspace root:

- a heap (bump allocator) behind the game's allocators: `operator new`
  0x59eff0 / 0x59ef90 (cdecl size), the pool allocator 0x4211d0 (cdecl size,
  pool), `free` 0x59ecd0 (ignored) and the critical sections 0x5322b0/0x5322c0
  (no-ops);
- the track node graph `[0x628bc0]` built by the original code: 0x47fd30
  allocates the object, `TrackNodes_Build` 0x480140 (thiscall: node count,
  section count, SimD nodes, SimT section lengths) builds sections, junctions
  and the node lookup tables, `TrackNodes_LoadJunctionWeights` 0x481080
  (thiscall: path) reads the `.jnc` weights through the file stubs 0x59ddd0
  (size) / 0x59d8e0 (load) / 0x531f90 (free);
- the racing line object `[0x5e671c]`: `+0x58` points at the `.lsp` (x, z)
  pairs (`0x4033d0`: file data + 4).

Track CRPs are RefPack-compressed (`10 fb`); the SimD/SimT chunks are found
in the decompressed `karT` table (entry: tag, flags, count, offset; flags
bit 1 makes the offset relative to the entry).
"""
from pathlib import Path
import struct

from unicorn.x86_const import UC_X86_REG_EAX

from vehicle_harness import Machine, load_module, ROOT  # noqa: F401  (re-exported)

HEAP = 0x3000000
HEAP_SIZE = 0x3000000
FILES = 0x6000000          # stubbed file contents
FILES_SIZE = 0x400000

CPU_3DNOW, CPU_SSE = 0x5deb20, 0x5deb1c
TRACK_NODES_PTR = 0x628bc0
RACING_LINE_PTR = 0x5e671c

ALLOC_NEW = 0x59eff0
ALLOC_NEW2 = 0x59ef90
ALLOC_POOL = 0x4211d0
ALLOC_LARGE = 0x59ecb0
FREE = 0x59ecd0
FREE2 = 0x531f90
FREE3 = 0x59f050
CS_ENTER = 0x5322b0
CS_LEAVE = 0x5322c0
MATH_ERROR = 0x5a4e97
FILE_SIZE = 0x59ddd0
FILE_LOAD = 0x59d8e0

# CRT static initialisers of the junction search radii: [0x628bb8] = 90²,
# [0x628bbc] = 18² (0x5b47bc, 0x5b47c0).
TRACKNODES_STATIC_INIT = (0x47fcf0, 0x47fd10)
TRACKNODES_NEW = 0x47fd30
TRACKNODES_BUILD = 0x480140
TRACKNODES_JNC = 0x481080

NODE_SIZE = 0x50
SECTION_SIZE = 0x28
JUNCTION_SIZE = 0x38


def refpack(d):
    """EA RefPack (0x10fb) decompression."""
    flags = d[0]
    p = 2
    if flags & 0x01:
        p += 3
    out_size = int.from_bytes(d[p:p + 3], 'big')
    p += 3
    out = bytearray()
    while p < len(d):
        b0 = d[p]
        if b0 < 0x80:
            b1 = d[p + 1]
            p += 2
            lit, ln, off = b0 & 3, ((b0 >> 2) & 7) + 3, ((b0 & 0x60) << 3) + b1 + 1
        elif b0 < 0xc0:
            b1, b2 = d[p + 1], d[p + 2]
            p += 3
            lit, ln, off = b1 >> 6, (b0 & 0x3f) + 4, ((b1 & 0x3f) << 8) + b2 + 1
        elif b0 < 0xe0:
            b1, b2, b3 = d[p + 1], d[p + 2], d[p + 3]
            p += 4
            lit = b0 & 3
            ln = ((b0 & 0x0c) << 6) + b3 + 5
            off = ((b0 & 0x10) << 12) + (b1 << 8) + b2 + 1
        elif b0 < 0xfc:
            lit, ln = ((b0 & 0x1f) << 2) + 4, 0
            p += 1
        else:
            lit, ln = b0 & 3, 0
            p += 1
        out += d[p:p + lit]
        p += lit
        for _ in range(ln):
            out.append(out[-off])
        if b0 >= 0xfc:
            break
    return bytes(out[:out_size])


def track_crp(path):
    data = Path(path).read_bytes()
    if data[1] == 0xfb and data[0] & 0xfe == 0x10:
        data = refpack(data)
    if data[:4] != b'karT':
        raise ValueError(f'{path}: not a track CRP')
    return data


def crp_chunk(crp, tag):
    """(count, offset) of the first table entry with `tag` (e.g. b'SimD')."""
    fourcc = tag[::-1]
    i = crp.find(fourcc)
    while i >= 0 and i % 4:
        i = crp.find(fourcc, i + 1)
    if i < 0:
        raise KeyError(tag)
    flags, count, off = struct.unpack_from('<III', crp, i + 4)
    if flags & 2:
        off += i
    return count, off


def sim_nodes(crp):
    """Raw SimD node records and SimT section lengths of a track CRP."""
    n, off = crp_chunk(crp, b'SimD')
    nodes = crp[off:off + n * NODE_SIZE]
    try:
        ns, soff = crp_chunk(crp, b'SimT')
        sections = list(struct.unpack_from(f'<{ns}i', crp, soff))
    except KeyError:
        sections = []
    return n, nodes, sections


class AiMachine(Machine):
    """Machine with a heap, allocator/file stubs and track-graph helpers."""

    def __init__(self, module, data, stubs=None, **kw):
        base_stubs = {
            ALLOC_NEW: lambda m: m.alloc(m.arg(0)),
            ALLOC_NEW2: lambda m: m.alloc(m.arg(0)),
            ALLOC_POOL: lambda m: m.alloc(m.arg(0)),
            ALLOC_LARGE: lambda m: m.alloc(m.arg(0)),
            FREE: lambda m: 0,
            FREE2: lambda m: 0,
            FREE3: lambda m: 0,
            CS_ENTER: lambda m: 0,
            CS_LEAVE: lambda m: 0,
            FILE_SIZE: lambda m: len(m.file_data),
            FILE_LOAD: lambda m: m.file_addr,
            # CRT math error report `_87except`-style 0x5a4e97 (acos of |x| > 1
            # from rounding in 0x40b650): it reaches _matherr/errno through
            # imports; skipped, so the x87 result stays the indefinite NaN the
            # caller loaded (0x5e5870). **Hypothesis:** the game returns it too.
            MATH_ERROR: lambda m: m.uc.reg_read(UC_X86_REG_EAX),
        }
        base_stubs.update(stubs or {})
        super().__init__(module, data, stubs=base_stubs, **kw)
        # Admit PACODE (0x5b0000..): Vec_TransformByMatrix and memcpy-style
        # helpers dispatch there; zero the 3DNow!/SSE feature bytes so the x87
        # kernels run (as replay-camera.py does).
        base = int(module['image_base'], 16)
        pacode = next(s for s in module['sections'] if s['name'] == 'PACODE')
        self.code = (self.code[0], base + pacode['rva'] + pacode['vsize'])
        self.uc.mem_write(CPU_3DNOW, b'\0')
        self.uc.mem_write(CPU_SSE, b'\0')
        self.uc.mem_map(HEAP, HEAP_SIZE)
        self.uc.mem_map(FILES, FILES_SIZE)
        self.heap = HEAP
        self.file_data = b''
        self.file_addr = FILES
        self.files_top = FILES

    def arg(self, i):
        """Stack argument i of a stubbed cdecl call (at the stub's entry)."""
        from unicorn.x86_const import UC_X86_REG_ESP
        sp = self.uc.reg_read(UC_X86_REG_ESP)
        return struct.unpack('<I', self.uc.mem_read(sp + 4 + 4 * i, 4))[0]

    def alloc(self, size):
        size = max(int(size), 1)
        addr = self.heap
        self.heap = (self.heap + size + 15) & ~15
        if self.heap > HEAP + HEAP_SIZE:
            raise RuntimeError('harness heap exhausted')
        self.uc.mem_write(addr, bytes(size))
        return addr

    def place_file(self, data):
        """Copies `data` into the file area; returns its address."""
        addr = self.files_top
        self.files_top = (addr + len(data) + 15) & ~15
        if self.files_top > FILES + FILES_SIZE:
            raise RuntimeError('harness file area exhausted')
        self.uc.mem_write(addr, bytes(data))
        return addr

    # -- track graph ---------------------------------------------------------
    def build_track(self, crp, jnc=None):
        """Runs 0x47fd30, 0x480140 and (with `jnc`) 0x481080 like the track
        loader 0x4876e0 does; returns the TrackNodes object address."""
        n, nodes, sections = sim_nodes(crp)
        for init in TRACKNODES_STATIC_INIT:
            self.call(init, budget=1000)
        self.call(TRACKNODES_NEW, budget=100_000)
        obj = self.get_u32(TRACK_NODES_PTR)
        nodes_addr = self.place_file(nodes)
        sec_addr = self.place_file(struct.pack(f'<{len(sections)}i', *sections)) if sections else 0
        self.call_thiscall(TRACKNODES_BUILD, obj, n, len(sections), nodes_addr, sec_addr,
                           budget=400_000_000)
        if jnc is not None:
            self.file_data = bytes(jnc)
            self.file_addr = self.place_file(jnc)
            path = self.place_file(b'track.jnc\0')
            self.call_thiscall(TRACKNODES_JNC, obj, path, budget=10_000_000)
        return obj

    def set_racing_line(self, lsp):
        """Racing line object at [0x5e671c] with +0x58 -> lsp data + 4."""
        obj = self.alloc(0x5c)
        data = self.place_file(lsp)
        self.put_u32(obj + 0x4c, data)
        self.put_u32(obj + 0x58, data + 4)
        self.put_u32(RACING_LINE_PTR, obj)
        return obj

    def vec(self, addr, stride):
        """(begin, count) of an MSVC vector {begin, end, cap} at addr."""
        begin, end = self.get_u32(addr), self.get_u32(addr + 4)
        return begin, (end - begin) // stride

    def dump_track(self, obj):
        """Track graph as plain data: node count, lap length +0x4 and last lap
        node +0x8, the byte tables +0x10/+0x14,
        sections (+0x18), junctions (+0x1c) with endpoints as (section, end),
        near-node lists (+0x20)."""
        count, lap_nodes, lap_end = self.get_i32(obj, 3)
        tab10 = self.get_bytes(self.get_u32(obj + 0x10), count)
        tab14 = self.get_bytes(self.get_u32(obj + 0x14), count)
        sec_begin, nsec = self.vec(self.get_u32(obj + 0x18), SECTION_SIZE)
        jun_begin, njun = self.vec(self.get_u32(obj + 0x1c), JUNCTION_SIZE)

        def endpoint(ptr):
            if ptr == 0:
                return None
            k = ptr - sec_begin
            if not (0 <= k < nsec * SECTION_SIZE) or k % SECTION_SIZE not in (0, 0x10):
                raise RuntimeError(f'endpoint {ptr:#x} outside the section table')
            return (k // SECTION_SIZE, (k % SECTION_SIZE) // 0x10)

        def junction(ptr):
            if ptr == 0:
                return -1
            k = ptr - jun_begin
            if not (0 <= k < njun * JUNCTION_SIZE) or k % JUNCTION_SIZE:
                raise RuntimeError(f'junction {ptr:#x} outside the junction table')
            return k // JUNCTION_SIZE

        sections = []
        for i in range(nsec):
            a = sec_begin + i * SECTION_SIZE
            ends = []
            for e in range(2):
                node, sec, weight_bits, end = struct.unpack('<iiIi', self.get_bytes(a + 0x10 * e, 16))
                ends.append((node, sec, weight_bits, end))
            sections.append((ends, junction(self.get_u32(a + 0x20)), junction(self.get_u32(a + 0x24))))
        junctions = []
        for j in range(njun):
            a = jun_begin + j * JUNCTION_SIZE
            n = self.get_i32(a + 0x20)[0]
            eps = [endpoint(self.get_u32(a + 4 * k)) for k in range(n)]
            rest = struct.unpack('<IIfff', self.get_bytes(a + 0x24, 20))

            def slot(ptr):
                try:
                    return endpoint(ptr)
                except RuntimeError:
                    return 'x'
            slots = [slot(self.get_u32(a + 4 * k)) for k in range(8)]
            junctions.append((eps, rest, slots))
        near = []
        lists_begin, nlists = self.vec(self.get_u32(obj + 0x20), 4)
        for k in range(nlists):
            v = self.get_u32(lists_begin + 4 * k)
            b, c = self.vec(v, 8)
            near.append([struct.unpack('<iI', self.get_bytes(b + 8 * i, 8)) for i in range(c)])
        return {'count': count, 'lap_nodes': lap_nodes, 'lap_end': lap_end, 'tab10': tab10, 'tab14': tab14, 'sections': sections,
                'junctions': junctions, 'near': near}
