"""Unicorn harness for the original career/garage/market code of nfs5.exe.

Extends vehicle_harness.Machine with what the front-end side of the career needs:

- a heap: the allocators (0x59ef90 / 0x59eff0 / 0x59efc0 / 0x59ecb0 through the allocator
  table [0x5e4fe8], and the STL pool 0x4211d0) are stubbed with a bump allocator returning
  zeroed blocks; the frees (0x59f050, 0x59ecd0, 0x4237d0) do nothing; [0x5e4fe8] points at a
  zeroed scratch block so the inlined pool frees of std::vector growth write harmlessly;
  the critical-section wrappers 0x5322b0 / 0x5322c0 are stubbed;
- MSVC std::vector<T*> helpers ({begin, end, capacity} headers);
- fake front-end accessors: `FEAccessor_Find` 0x4b7ab0(name) returns the name pointer and
  `__RTDynamicCast` 0x5a134b(p, ...) maps it to a fake object whose vtable slots are stubs
  (thiscall, callee pops), or NULL;
- thiscall-aware stubs: a stub may name how many argument bytes the callee pops.

Synthetic catalogue/parts generators (layouts from workspace docs/formats/car-catalogue.md,
docs/re/career.md) are used by the recorders that commit their fixtures.
"""
import random
import struct
import sys
from pathlib import Path

from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ESP, UC_X86_REG_EIP

sys.path.insert(0, str(Path(__file__).resolve().parent))
from vehicle_harness import EXIT, Machine  # noqa: E402

HEAP, HEAP_SIZE = 0x4000000, 0x2000000
STUB_PAGE = 0x3f00000          # fake vtable targets
FAKE_OBJECTS = 0x3f10000       # fake accessor objects (0x100 each)
POOL_SCRATCH = 0x3f20000       # what [0x5e4fe8] points at
DATA = 0x3f40000               # catalogue, parts, misc (0xc0000)
DATA_SIZE = 0xc0000

CAR_RECORD = 0x670
PART_RECORD = 0x168
OWNED_CAR = 0x760

ALLOC = {0x59ef90: 0, 0x59eff0: 0, 0x59efc0: 0, 0x4211d0: 0, 0x59ecb0: 0}   # arg index of size
FREE = (0x59f050, 0x59ecd0, 0x4237d0)
LOCKS = (0x5322b0, 0x5322c0)


class CareerMachine(Machine):
    def __init__(self, module, data, stubs=None):
        self.pops = {}
        self.trace = []
        super().__init__(module, data, stubs=stubs)
        uc = self.uc
        uc.mem_map(STUB_PAGE, 0x10000)
        uc.mem_map(FAKE_OBJECTS, 0x10000)
        uc.mem_map(POOL_SCRATCH, 0x20000)
        uc.mem_map(DATA, DATA_SIZE)
        uc.mem_map(HEAP, HEAP_SIZE)
        uc.mem_write(STUB_PAGE, b'\xc3' * 0x10000)
        self.heap = HEAP
        self.data_top = DATA
        self.next_stub = STUB_PAGE
        self.next_object = FAKE_OBJECTS
        self.put_u32(0x5e4fe8, POOL_SCRATCH)
        for va, k in ALLOC.items():
            self.stubs[va] = self._alloc_stub(k)
        for va in FREE + LOCKS:
            self.stubs[va] = lambda _m: 0
        self.casts = {}                 # accessor name address -> fake object
        self.stubs[0x4b7ab0] = lambda m: m.arg(0)
        self.stubs[0x5a134b] = lambda m: m.casts.get(m.arg(0), 0)

    # -- stubs -------------------------------------------------------------
    def _guard(self, uc, address, size, _user):
        if address in self.stubs:
            eax = self.stubs[address](self)
            sp = uc.reg_read(UC_X86_REG_ESP)
            ret = struct.unpack('<I', uc.mem_read(sp, 4))[0]
            uc.reg_write(UC_X86_REG_ESP, sp + 4 + self.pops.get(address, 0))
            uc.reg_write(UC_X86_REG_EAX, (eax or 0) & 0xffffffff)
            uc.reg_write(UC_X86_REG_EIP, ret)
            return
        if not (self.code[0] <= address < self.code[1] or address == EXIT):
            raise RuntimeError(f'execution left .text: {address:08x} (stub needed); '
                               f'last calls {[hex(a) for a in self.trace]}')
        if size == 5 and uc.mem_read(address, 1)[0] == 0xe8:
            target = (address + 5 + struct.unpack('<i', uc.mem_read(address + 1, 4))[0]) & 0xffffffff
            self.trace = (self.trace + [address, target])[-24:]

    def arg(self, k):
        """k-th dword argument of the stubbed call (return address at [esp])."""
        sp = self.uc.reg_read(UC_X86_REG_ESP)
        return struct.unpack('<I', self.uc.mem_read(sp + 4 + 4 * k, 4))[0]

    def _alloc_stub(self, k):
        def h(m):
            return m.alloc(m.arg(k))
        return h

    def alloc(self, size):
        size = max(1, size)
        p = self.heap
        self.heap = (self.heap + size + 15) & ~15
        if self.heap > HEAP + HEAP_SIZE:
            raise RuntimeError('heap exhausted')
        self.uc.mem_write(p, bytes(size))
        return p

    def data_block(self, raw):
        p = self.data_top
        self.data_top = (self.data_top + len(raw) + 15) & ~15
        if self.data_top > DATA + DATA_SIZE:
            raise RuntimeError('data area exhausted')
        self.uc.mem_write(p, bytes(raw))
        return p

    def stub_at(self, handler, pops=0):
        """A fresh code address that runs `handler` (thiscall: `pops` argument bytes)."""
        va = self.next_stub
        self.next_stub += 1
        self.stubs[va] = handler
        self.pops[va] = pops
        return va

    def fake_object(self, slots):
        """Object with a vtable; slots = {offset: (handler, pops)}."""
        obj = self.next_object
        self.next_object += 0x400
        vt = obj + 0x100
        self.put_u32(obj, vt)
        for o, (h, pops) in slots.items():
            self.put_u32(vt + o, self.stub_at(h, pops))
        return obj

    # -- std::vector<T*> ---------------------------------------------------
    def vector(self, items, capacity=None):
        cap = max(capacity or len(items), len(items))
        body = self.alloc(4 * max(cap, 1))
        for k, v in enumerate(items):
            self.put_u32(body + 4 * k, v)
        hdr = self.alloc(12)
        self.put_u32(hdr, body)
        self.put_u32(hdr + 4, body + 4 * len(items))
        self.put_u32(hdr + 8, body + 4 * cap)
        return hdr

    def vector_items(self, hdr):
        b, e = self.get_u32(hdr), self.get_u32(hdr + 4)
        return [self.get_u32(p) for p in range(b, e, 4)]

    def global_vector(self, va, items, capacity=None):
        self.put_u32(va, self.vector(items, capacity))

    def i16(self, addr):
        return struct.unpack('<h', self.get_bytes(addr, 2))[0]

    def cstr(self, addr, limit=260):
        raw = self.get_bytes(addr, limit)
        return raw.split(b'\0')[0].decode('latin1')


def fnv64(data):
    h = 0xcbf29ce484222325
    for b in data:
        h = ((h ^ b) * 0x100000001b3) & 0xffffffffffffffff
    return h


# ---------------------------------------------------------------------------
# Synthetic data (no game content): layouts only.
PART_NAMES = {1: ['Engine', 'Engine (Standard)', 'Exhaust', 'Turbo', 'Air Intake'], 2: ['Brakes'],
              3: ['Gear Box', 'Shifter'], 4: ['Springs'], 5: ['Shocks'], 6: ['Spoiler'],
              7: ['Body', 'Doors', 'Hood'], 8: ['Sway Bars'], 9: [''], 10: ['Body', 'Rims', 'Roof']}


def synthetic_parts(seed=11, n=110):
    rng = random.Random(seed)
    out = []
    for k in range(n):
        r = bytearray(PART_RECORD)
        struct.pack_into('<h', r, 0x00, 1000 + k)
        r[0x08:0x08 + 8] = f'M{k % 23:02d}'.encode().ljust(8, b'\0')
        cat = rng.choice(list(PART_NAMES))
        struct.pack_into('<h', r, 0x26, cat)
        name = rng.choice(PART_NAMES[cat]).encode()
        r[0x28:0x28 + len(name)] = name
        struct.pack_into('<h', r, 0x46, rng.randint(1948, 2001))
        struct.pack_into('<i', r, 0x5c, rng.choice((0, 150, 284, 900, 1200, 4876, 22500)) + rng.randint(0, 99))
        struct.pack_into('<f', r, 0x164, float(rng.choice((0, 0, 10, 37, 100))))
        for o in range(0x60, 0x160, 4):          # other fields: noise, carried along by copies
            if rng.random() < 0.2:
                struct.pack_into('<i', r, o, rng.randint(-5, 5000))
        out.append(bytes(r))
    return out


def synthetic_catalogue(parts, seed=13, n=32):
    rng = random.Random(seed)
    ids = [struct.unpack_from('<h', p, 0)[0] for p in parts]
    out = []
    for k in range(n):
        r = bytearray(CAR_RECORD)
        struct.pack_into('<h', r, 0x000, 60 + k)
        r[0x002:0x002 + 12] = f'Car {k:02d}'.encode().ljust(12, b'\0')
        r[0x07f:0x07f + 8] = f'm{k:02d}'.encode().ljust(8, b'\0')
        struct.pack_into('<i', r, 0x1ac, rng.choice((501, 502, 503)))
        first = rng.randint(1948, 2000)
        struct.pack_into('<i', r, 0x1b0, 521 if first < 1970 else 522 if first < 1989 else 523)
        struct.pack_into('<h', r, 0x1f4, 0 if rng.random() < 0.85 else rng.choice((1, 2)))
        struct.pack_into('<h', r, 0x1f8, first)
        struct.pack_into('<h', r, 0x1fc, rng.choice((0, 0, first - 1, first)))
        struct.pack_into('<h', r, 0x202, min(2000, first + rng.randint(0, 6)) if rng.random() < 0.95 else 0)
        struct.pack_into('<h', r, 0x204, rng.choice((0, 1, 1, 1, 2, 3)))
        struct.pack_into('<i', r, 0x208, rng.choice((11000, 12700, 30680, 65800, 137030, 250000)) + rng.randint(0, 999))
        v = 100.0
        for a in range(56):
            struct.pack_into('<f', r, 0x20c + 4 * a, v)
            v = max(20.0, v - rng.choice((0, 0, 1, 2.5, 5)))
        market = rng.sample(ids, rng.randint(0, 40))
        struct.pack_into('<h', r, 0x32e, len(market))
        for j, pid in enumerate(market):
            struct.pack_into('<h', r, 0x330 + 2 * j, pid)
        stock = rng.sample(ids, rng.randint(0, 17))
        struct.pack_into('<h', r, 0x3bc, len(stock))
        for j, pid in enumerate(stock):
            struct.pack_into('<h', r, 0x3be + 2 * j, pid)
        ncol = rng.randint(0, 6)
        struct.pack_into('<h', r, 0x3e6, ncol)
        for j in range(ncol):
            for o in (0x3ec, 0x3f0, 0x3f4):
                struct.pack_into('<I', r, o + 0x10 * j, rng.getrandbits(32))
        nint = rng.randint(0, 5)
        struct.pack_into('<h', r, 0x528, nint)
        for j in range(nint):
            struct.pack_into('<I', r, 0x530 + 0x10 * j, rng.getrandbits(32))
        out.append(bytes(r))
    return out


def records(blob, size):
    return [blob[i:i + size] for i in range(0, len(blob) - size + 1, size)]
