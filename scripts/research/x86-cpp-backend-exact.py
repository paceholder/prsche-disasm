"""Run 431: static x86 lowering with integer semantics and EFLAGS exact against hardware.

Inherits the frozen x86-cpp-backend-fpu.py (SHA-checked; it stays untouched because existing
validations pin its hash) and fixes what hardware oracles found while lowering the whole game
(an experiment in the contributor's fork), in C++ for the MSVC x86 runtime:

- PF and AF: the parity of the result's low byte for every flag-writing helper; AF is the carry/
  borrow out of bit 3 for add/adc/sub/sbb/cmp/neg/inc/dec and 0 for logical operations and shifts.
- MUL/IMUL: the flags Intel leaves undefined are those of the target CPU (Intel Core i3-3220):
  SF from the sign and PF from the parity of the (low) result, ZF and AF cleared.
- Shifts and rotates (shl/shr/sar/rol/ror/rcl/rcr; 32-bit shld/shrd), measured on the target CPU for every
  width, count 0..31 and immediate/CL form: a zero (masked) count changes nothing; otherwise OF is
  the OF of the first single-bit step, except ROL/ROR with an immediate count above 1, which keep
  OF. Shifts set CF to the last bit out, SF/ZF/PF from the result and clear AF; rotates change
  only CF and OF (RCL/RCR change nothing when the count is a multiple of width + 1).
- Semantic bugs: DIV r/m8 divides AX (it was lowered as a 32-bit DIV); LAHF sets bit 1; BT/BTS
  with 32-bit operands and a register offset use bit-string addressing; JP/JNP/JO/JNO exist;
  LOCK INC/DEC32 are atomic (identity-mapped host memory, interlocked add, same flags);
  CLD/STD set DF and every string step
  (ESI/EDI) follows it.

Unproved BT/BTS, LOCK INC/DEC and SHLD/SHRD widths are rejected. DIV8 inputs with a zero
divisor or an overflowing quotient stop as UnsupportedInput without GPR/flag writeback;
this tooling boundary does not model x86 #DE.

x87 stays the frozen binary64 model of x86-cpp-backend-fpu.py. Proof, on the target CPU:
scripts/research/verify-v2-x86-exact.py --set int|sse|branch|string and
scripts/research/verify-v2-x86-exact-lock.py.
"""
import hashlib
import importlib.util
from pathlib import Path

import capstone

BASELINE_SHA = '651faf47f14a432dcb7652d17b93ecbcfb3bf70d53d2dd6d21005afae7012a32'
BASELINE_PATH = Path(__file__).with_name('x86-cpp-backend-fpu.py')
if hashlib.sha256(BASELINE_PATH.read_bytes()).hexdigest() != BASELINE_SHA:
    raise ValueError('Frozen x86-cpp-backend-fpu.py SHA mismatch')

_spec = importlib.util.spec_from_file_location('frozen_fpu_for_exact', BASELINE_PATH)
_base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_base)

MUL_FLAGS = '(of ? 0x801u : 0u)'
HEADER_PATCHES = [
    # Exact-only unsupported-input boundary; preserve the frozen stop values.
    ('enum class Stop257:U {Running,Returned,MissingEntry,BadReturn,BudgetExhausted};',
     'enum class Stop257:U {Running,Returned,MissingEntry,BadReturn,BudgetExhausted,UnsupportedInput};'),
    # PF everywhere, AF cleared by default (logical operations, shifts).
    ('static inline void flags257(Machine257&s,U v,U bits,U cf,U of){\n'
     '    v&=mask257(bits);\n'
     '    s.flags=(s.flags&~0x8c1u)|(cf?1u:0u)|(v==0?0x40u:0u)|\n',
     'static inline U parity257(U v){v&=0xffu;v^=v>>4;v^=v>>2;v^=v>>1;return (~v&1u)<<2;}\n'
     'static inline void flags257(Machine257&s,U v,U bits,U cf,U of){\n'
     '    v&=mask257(bits);\n'
     '    s.flags=(s.flags&~0x8d5u)|parity257(v)|(cf?1u:0u)|(v==0?0x40u:0u)|\n'),
    # AF for add/adc/sub/sbb/cmp/neg.
    ('    flags257(s,v,bits,(sum>>bits)!=0,((~(a^b)&(a^v))>>(bits-1))&1u);return v;\n',
     '    flags257(s,v,bits,(sum>>bits)!=0,((~(a^b)&(a^v))>>(bits-1))&1u);s.flags|=(a^b^v)&0x10u;return v;\n'),
    ('        (((a^b)&(a^v))>>(bits-1))&1u);return v;\n',
     '        (((a^b)&(a^v))>>(bits-1))&1u);s.flags|=(a^b^v)&0x10u;return v;\n'),
    ('    flags257(s, v, bits, (sum >> bits) != 0, ((~(a ^ b) & (a ^ v)) >> (bits - 1)) & 1u);\n    return v;\n',
     '    flags257(s, v, bits, (sum >> bits) != 0, ((~(a ^ b) & (a ^ v)) >> (bits - 1)) & 1u);\n'
     '    s.flags |= (a ^ b ^ v) & 0x10u;\n    return v;\n'),
    # INC/DEC: PF and AF (CF unchanged).
    ('    U of = (a == (mask >> 1));\n'
     '    s.flags = (s.flags & ~0x8c0u) | (v == 0 ? 0x40u : 0u) | (((v >> (bits - 1)) & 1u) ? 0x80u : 0u) | (of ? 0x800u : 0u);\n',
     '    U of = (a == (mask >> 1));\n'
     '    s.flags = (s.flags & ~0x8d4u) | parity257(v) | ((a ^ v) & 0x10u) | (v == 0 ? 0x40u : 0u) | (((v >> (bits - 1)) & 1u) ? 0x80u : 0u) | (of ? 0x800u : 0u);\n'),
    ('    U of = (a == ((mask >> 1) + 1u));\n'
     '    s.flags = (s.flags & ~0x8c0u) | (v == 0 ? 0x40u : 0u) | (((v >> (bits - 1)) & 1u) ? 0x80u : 0u) | (of ? 0x800u : 0u);\n',
     '    U of = (a == ((mask >> 1) + 1u));\n'
     '    s.flags = (s.flags & ~0x8d4u) | parity257(v) | ((a ^ v) & 0x10u) | (v == 0 ? 0x40u : 0u) | (((v >> (bits - 1)) & 1u) ? 0x80u : 0u) | (of ? 0x800u : 0u);\n'),
    # MUL/IMUL: undefined SF/ZF/AF/PF as the target CPU sets them.
    ('    U of = ((long long)(int)v != res);\n    s.flags = (s.flags & ~0x801u) | ' + MUL_FLAGS + ';\n',
     '    U of = ((long long)(int)v != res);\n'
     '    s.flags = (s.flags & ~0x8d5u) | ' + MUL_FLAGS + ' | parity257(v) | (v & 0x80000000u ? 0x80u : 0u);\n'),
    ('    U of = ((long long)(int)s.r[0] != res);\n    s.flags = (s.flags & ~0x801u) | ' + MUL_FLAGS + ';\n',
     '    U of = ((long long)(int)s.r[0] != res);\n'
     '    s.flags = (s.flags & ~0x8d5u) | ' + MUL_FLAGS + ' | parity257(s.r[0]) | (s.r[0] & 0x80000000u ? 0x80u : 0u);\n'),
    ('    U of = ((short)(signed char)(res & 0xff) != res);\n    s.flags = (s.flags & ~0x801u) | ' + MUL_FLAGS + ';\n',
     '    U of = ((short)(signed char)(res & 0xff) != res);\n'
     '    s.flags = (s.flags & ~0x8d5u) | ' + MUL_FLAGS + ' | parity257((U)res) | (res & 0x80 ? 0x80u : 0u);\n'),
    ('    U of = (s.r[2] != 0u);\n    s.flags = (s.flags & ~0x801u) | ' + MUL_FLAGS + ';\n',
     '    U of = (s.r[2] != 0u);\n'
     '    s.flags = (s.flags & ~0x8d5u) | ' + MUL_FLAGS + ' | parity257(s.r[0]) | (s.r[0] & 0x80000000u ? 0x80u : 0u);\n'),
    # DIV r/m8: reject fault inputs before division/writeback; no #DE model.
    ('static inline void div257_1(Machine257& s, U b) {\n',
     'static inline void div257_1_8(Machine257& s, U b) {\n'
     '    U ax = s.r[0] & 0xffffu; b &= 0xffu;\n'
     '    if (!b) { s.stop = Stop257::UnsupportedInput; return; }\n'
     '    U q = ax / b;\n'
     '    if (q > 0xffu) { s.stop = Stop257::UnsupportedInput; return; }\n'
     '    U r = ax % b;\n'
     '    s.r[0] = (s.r[0] & 0xffff0000u) | ((r & 0xffu) << 8) | (q & 0xffu);\n}\n'
     'static inline void div257_1(Machine257& s, U b) {\n'),
    # Shifts: OF of the first single-bit step for every count >= 1.
    ('    U v = (a << count) & mask;\n    U of = (count == 1u) ? (((v >> (bits - 1)) & 1u) ^ cf) : 0u;\n',
     '    U v = (a << count) & mask;\n    U of = ((a >> (bits - 1)) ^ (a >> (bits - 2))) & 1u;\n'),
    ('    U v = (a >> count) & mask;\n    U of = (count == 1u) ? ((a >> (bits - 1)) & 1u) : 0u;\n',
     '    U v = (a >> count) & mask;\n    U of = (a >> (bits - 1)) & 1u;\n'),
]

ROTATES = r'''
static inline U rotflags257(Machine257& s, U cf, U of) {
    s.flags = (s.flags & ~0x801u) | (cf ? 1u : 0u) | (of ? 0x800u : 0u); return 0u;
}
// ROL/ROR: CF from the result; OF of the first single-bit step, kept for an immediate count > 1.
static inline U rol257(Machine257& s, U a, U count, U bits, U imm) {
    U mask = mask257(bits); a &= mask; U n = count & 0x1fu;
    if (!n) return a;
    U k = n % bits; U v = k ? (((a << k) | (a >> (bits - k))) & mask) : a;
    U of = (imm && n > 1u) ? ((s.flags >> 11) & 1u) : (((a >> (bits - 1)) ^ (a >> (bits - 2))) & 1u);
    rotflags257(s, v & 1u, of); return v;
}
static inline U ror257(Machine257& s, U a, U count, U bits, U imm) {
    U mask = mask257(bits); a &= mask; U n = count & 0x1fu;
    if (!n) return a;
    U k = n % bits; U v = k ? (((a >> k) | (a << (bits - k))) & mask) : a;
    U of = (imm && n > 1u) ? ((s.flags >> 11) & 1u) : ((a ^ (a >> (bits - 1))) & 1u);
    rotflags257(s, (v >> (bits - 1)) & 1u, of); return v;
}
// RCL/RCR through CF; a count that is a multiple of width + 1 changes nothing.
static inline U rcl257(Machine257& s, U a, U count, U bits) {
    U mask = mask257(bits); a &= mask; U n = (count & 0x1fu) % (bits + 1u);
    if (!n) return a;
    U cf = s.flags & 1u; U of = ((a >> (bits - 1)) ^ (a >> (bits - 2))) & 1u;
    for (U k = 0; k < n; ++k) { U out = (a >> (bits - 1)) & 1u; a = ((a << 1) | cf) & mask; cf = out; }
    rotflags257(s, cf, of); return a;
}
static inline U rcr257(Machine257& s, U a, U count, U bits) {
    U mask = mask257(bits); a &= mask; U n = (count & 0x1fu) % (bits + 1u);
    if (!n) return a;
    U cf = s.flags & 1u; U of = (cf ^ (a >> (bits - 1))) & 1u;
    for (U k = 0; k < n; ++k) { U out = a & 1u; a = (a >> 1) | (cf << (bits - 1)); cf = out; }
    rotflags257(s, cf, of); return a;
}
// SHLD/SHRD: translation accepts only 32-bit operands, so every masked count is safe.
static inline U shld257(Machine257& s, U a, U b, U count, U bits) {
    U mask = mask257(bits); a &= mask; b &= mask; U n = count & 0x1fu;
    if (!n) return a;
    U v = ((a << n) | (b >> (bits - n))) & mask;
    U cf = (a >> (bits - n)) & 1u; U of = ((a >> (bits - 1)) ^ (a >> (bits - 2))) & 1u;
    flags257(s, v, bits, cf, of); return v;
}
static inline U shrd257(Machine257& s, U a, U b, U count, U bits) {
    U mask = mask257(bits); a &= mask; b &= mask; U n = count & 0x1fu;
    if (!n) return a;
    U v = ((a >> n) | (b << (bits - n))) & mask;
    U cf = (a >> (n - 1)) & 1u; U of = (b ^ (a >> (bits - 1))) & 1u;
    flags257(s, v, bits, cf, of); return v;
}
'''

BT_MEM = ("            if ops[0].size != 4:\n"
          "                raise ValueError('Unsupported bt/bts width: only 32-bit operands proved')\n"
          "            if ops[0].type == X86_OP_MEM and ops[1].type == X86_OP_REG:\n"
          "                # Bit-string addressing: a register offset may leave the dword (signed).\n"
          "                code = (f'U off = {expression(i, ops[1])}; U a = {address(i, ops[0])} + (U)(((int)off >> 5) * 4); '\n"
          "                        f'U bit = 1u << (off & 31u); U val = read257(a, 4u); '\n"
          "                        f's.flags = (s.flags & ~1u) | ((val & bit) ? 1u : 0u);' + ({extra}))\n"
          "            else:\n")

SOURCE_PATCHES = [
    ("branches = ('jl','jge','je','jz','jne','jnz','jg','jle','ja','jbe','jb','jae','js','jns','jecxz')",
     "branches = ('jl','jge','je','jz','jne','jnz','jg','jle','ja','jbe','jb','jae','js','jns','jecxz',"
     "'jp','jnp','jo','jno')"),
    ("            elif m=='jns': condition='!(s.flags&0x80u)'\n",
     "            elif m=='jns': condition='!(s.flags&0x80u)'\n"
     "            elif m=='jp': condition='(s.flags&0x4u)'\n"
     "            elif m=='jnp': condition='!(s.flags&0x4u)'\n"
     "            elif m=='jo': condition='(s.flags&0x800u)'\n"
     "            elif m=='jno': condition='!(s.flags&0x800u)'\n"),
    ("            code = 's.r[0] = (s.r[0] & 0xffff00ffu) | ((s.flags & 0xd5u) << 8u);'",
     "            code = 's.r[0] = (s.r[0] & 0xffff00ffu) | (((s.flags & 0xd5u) | 2u) << 8u);'"),
    ("            code = f'div257_1(s, {expression(i, ops[0])});'",
     "            code = (f'div257_1_8(s, {expression(i, ops[0])});if(s.stop!=Stop257::Running)return;' if ops[0].size == 1\n"
     "                    else f'div257_1(s, {expression(i, ops[0])});')"),
    ("        elif m == 'bts' and len(ops) == 2:\n            code = ",
     "        elif m == 'bts' and len(ops) == 2:\n" + BT_MEM.replace('{extra}', "' write257(a, 4u, val | bit);'") + "                code = "),
    ("        elif m == 'bt' and len(ops) == 2:\n            code = ",
     "        elif m == 'bt' and len(ops) == 2:\n" + BT_MEM.replace('{extra}', "''") + "                code = "),
    ("            fn = 'inc257' if 'inc' in m else 'dec257'\n"
     "            code = f'U a={expression(i,ops[0])};U value={fn}(s,a,{bits}u);' + store(i, ops[0], 'value')",
     "            fn = 'inc257' if 'inc' in m else 'dec257'\n"
     "            if m.startswith('lock') and bits != 32:\n"
     "                raise ValueError('Unsupported locked inc/dec width: only 32-bit operands proved')\n"
     "            if m.startswith('lock') and ops[0].type == X86_OP_MEM and bits == 32:\n"
     "                delta = '1' if 'inc' in m else '-1'\n"
     "                code = (f'U old=static_cast<U>(_InterlockedExchangeAdd(reinterpret_cast<volatile long*>({address(i,ops[0])}),{delta}));'\n"
     "                        f'U value={fn}(s,old,32u);(void)value;')\n"
     "            else:\n"
     "                code = f'U a={expression(i,ops[0])};U value={fn}(s,a,{bits}u);' + store(i, ops[0], 'value')"),
    ("        elif m in ('rol', 'ror') and len(ops) == 2:\n            sh_left = (m == 'rol')\n            code = ",
     "        elif m in ('rol', 'ror') and len(ops) == 2 and ops[0].size in (1, 2, 4):\n"
     "            imm = 1 if ops[1].type == X86_OP_IMM else 0\n"
     "            code = (f'U value={m}257(s,{expression(i, ops[0])},{expression(i, ops[1])},{ops[0].size * 8}u,{imm}u);'\n"
     "                    + store(i, ops[0], 'value'))\n"
     "        elif m in ('rcl', 'rcr') and len(ops) == 2 and ops[0].size in (1, 2, 4):\n"
     "            code = (f'U value={m}257(s,{expression(i, ops[0])},{expression(i, ops[1])},{ops[0].size * 8}u);'\n"
     "                    + store(i, ops[0], 'value'))\n"
     "        elif False:\n            code = "),
    ("        elif m == 'shld' and len(ops) == 3:\n            code = ",
     "        elif m in ('shld', 'shrd') and len(ops) == 3:\n"
     "            if ops[0].size != 4:\n"
     "                raise ValueError('Unsupported shld/shrd width: only 32-bit operands proved')\n"
     "            code = (f'U value={m}257(s,{expression(i, ops[0])},{expression(i, ops[1])},{expression(i, ops[2])},{ops[0].size * 8}u);'\n"
     "                    + store(i, ops[0], 'value'))\n"
     "        elif False:\n            code = "),
]


import re

# DF: CLD/STD set the direction flag and every string step (ESI/EDI += size) follows it.
SOURCE_PATCHES.append(("        elif m in ('nop', 'cld', 'std'):\n            code = ''\n",
                       "        elif m == 'cld':\n            code = 's.flags&=~0x400u;'\n"
                       "        elif m == 'std':\n            code = 's.flags|=0x400u;'\n"
                       "        elif m in ('nop', 'prefetch', 'prefetchw', 'prefetchnta', 'prefetcht0', 'prefetcht1', 'prefetcht2'):\n"
                       "            code = ''\n"))
# PREFETCH*: cache hints without architectural effect.
_DF_STEP = re.compile(r's\.r\[([67])\] \+= ([124])u;')


def _df(text):
    return _DF_STEP.sub(lambda m: f's.r[{m.group(1)}] += (s.flags & 0x400u) ? 0u-{m.group(2)}u : {m.group(2)}u;', text)


def _apply(text, patches, what):
    for before, after in patches:
        if text.count(before) != 1:
            raise ValueError(f'{what} patch context drift: {before[:80]!r}')
        text = text.replace(before, after)
    return text


HEADER = _df(_apply(_base.HEADER, HEADER_PATCHES, 'header'))
_marker = 'static inline U shl257(Machine257& s, U a, U count, U bits) {'
HEADER = HEADER.replace(_marker, ROTATES + _marker) if HEADER.count(_marker) == 1 else None
if HEADER is None:
    raise ValueError('header shift marker drift')
if '#include <intrin.h>' not in HEADER:
    HEADER = HEADER.replace('#pragma once\n', '#pragma once\n#include <intrin.h>\n', 1)

DERIVED_SOURCE = _df(_apply(_base.DERIVED_SOURCE, SOURCE_PATCHES, 'source'))
_namespace = dict(_base._namespace)
_namespace['capstone'] = capstone
exec(compile(DERIVED_SOURCE, str(Path(__file__)) + '::derived', 'exec'), _namespace)
translate = _namespace['translate']
