"""Run 431: hardware oracle for x86-cpp-backend-exact.py on the target CPU (Intel Core i3-3220).

Every distinct instruction form Porsche.exe uses (mnemonic + operand kinds, from the catalogued
code) becomes a fragment: the original instruction with its memory operand re-encoded as [esi] and
as [esi+ebx*2+0x10], followed by RET (--set branch: every short Jcc 70..7f choosing between two
results). Each fragment runs natively and through the backend from the same random state (GPRs,
EFLAGS, MXCSR, XMM0-7, a valid x87 image, a 512-byte buffer) and the results are compared: GPRs
(ESP excluded), EFLAGS & 0xcd5, memory, XMM and MXCSR. Sets: int (integer forms; x87/MMX/3DNow!
and control-flow/stack/string/time forms excluded), sse, branch. x87 stays the frozen binary64
model of x86-cpp-backend-fpu.py and is not part of this oracle.
Negative controls: --mutate no-parity / no-af must make forms fail.

Usage: py -3 scripts/research/verify-v2-x86-exact.py --set int|sse|branch [--cases N]
Output: local/experiments/v2-x86-exact/ and iterations/v2/001-original-recovery/runs/431-x86-backend-exact/<set>-oracle.json
"""
import argparse
import datetime
import hashlib
import importlib.util
import json
import os
import re
import subprocess
from pathlib import Path

import capstone

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / 'local/experiments/v2-x86-exact'
RUN = ROOT / 'iterations/v2/001-original-recovery/runs/431-x86-backend-exact'
CATALOG = ROOT / 'research/v2/binaries/Porsche.exe-ddd748fdbe6d/functions.jsonl'
ORIGINAL_SHA = 'ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39'


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


# Negative controls: each must make the oracle fail at least one form.
MUTANTS = {
    'no-parity': None,     # header: PF never set
    'no-af': None,         # header: AF never set
    'rot-of': None,        # header: ROL/ROR keep OF for every count (ignores the CL/immediate rule)
    'no-df': None,         # header: string helpers ignore DF
}


THREEDNOW = {'femms', 'pfmul', 'pfadd', 'pfsub', 'pfacc', 'pi2fd', 'pfrcp', 'pfrcpit1', 'pfrcpit2'}
GPR32 = {'eax': 0, 'ecx': 1, 'edx': 2, 'ebx': 3, 'esp': 4, 'ebp': 5, 'esi': 6, 'edi': 7}


class Backend:
    """x86-cpp-backend-exact.py plus what the harness needs (direct memory access, toolchain)."""
    THREEDNOW = THREEDNOW
    GPR32 = GPR32

    def __init__(self):
        spec = importlib.util.spec_from_file_location('exact431', ROOT / 'scripts/research/x86-cpp-backend-exact.py')
        self.mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.mod)

    @staticmethod
    def native_kind(i):
        """x87, MMX and RCPSS forms: outside this oracle (x87 is the frozen binary64 model)."""
        m = i.mnemonic
        if m in THREEDNOW or m.startswith('f') or m in ('wait', 'emms', 'rcpss'):
            return 'excluded'
        return 'excluded' if any(op.type == 1 and i.reg_name(op.reg).startswith('mm') for op in i.operands) else None

    def backend(self):
        header = self.mod.HEADER
        for decl, impl in (('U read257(U address, U bytes);\n',
                            'static inline U read257(U a,U n){return n==1?*(const B*)a:n==2?*(const unsigned short*)a:*(const U*)a;}\n'),
                           ('void write257(U address, U bytes, U value);\n',
                            'static inline void write257(U a,U n,U v){if(n==1)*(B*)a=(B)v;else if(n==2)*(unsigned short*)a=(unsigned short)v;else *(U*)a=v;}\n')):
            assert header.count(decl) == 1, decl
            header = header.replace(decl, impl)
        return self.mod.translate, header

    @staticmethod
    def toolchain(vc_root=None):
        roots = [Path(r'C:\Program Files\Microsoft Visual Studio'), Path(r'C:\Program Files (x86)\Microsoft Visual Studio')]
        candidates = ([Path(vc_root).resolve() / 'bin/Hostx64/x86/cl.exe'] if vc_root else
                      list((ROOT / 'local/tools/BuildTools/VC/Tools/MSVC').glob('*/bin/Hostx64/x86/cl.exe')) +
                      [c for r in roots if r.exists() for c in r.glob('*/*/VC/Tools/MSVC/*/bin/Hostx64/x86/cl.exe')])
        candidates = sorted(c for c in candidates if c.is_file())
        if not candidates:
            raise ValueError('MSVC x86 not found; pass --vc-root with the MSVC version directory')
        cl = candidates[-1]
        kits = Path(r'C:\Program Files (x86)\Windows Kits\10')
        sdk_inc = sorted(q for q in (kits / 'Include').iterdir() if (q / 'um').exists())[-1]
        vc = cl.parents[3]
        env = dict(os.environ)
        env['INCLUDE'] = ';'.join(str(q) for q in [vc / 'include', sdk_inc / 'ucrt', sdk_inc / 'shared', sdk_inc / 'um'])
        env['LIB'] = ';'.join(str(q) for q in [vc / 'lib/x86', kits / 'Lib' / sdk_inc.name / 'ucrt/x86',
                                                kits / 'Lib' / sdk_inc.name / 'um/x86'])
        return cl, cl.parent / 'link.exe', env


def load_build():
    return Backend()


EXCLUDED_INT = {'push', 'pop', 'call', 'ret', 'jmp', 'leave', 'enter', 'int3', 'rdtsc', 'cpuid', 'pushal',
                'popal', 'pushfd', 'popfd', 'lock', 'rep', 'repne', 'repe', 'stosb', 'stosw', 'stosd', 'movsb',
                'movsw', 'movsd', 'lodsb', 'lodsd', 'scasb', 'cmpsb', 'xlatb', 'nop',
                'jecxz', 'ldmxcsr', 'stmxcsr', 'loop'}


def wanted(b, i, which):
    """fpu: the natively executed x87/MMX/RCPSS forms; int: every other data instruction whose
    operands the harness controls (no control flow, stack, string/rep, time or CPU-id, no ESP)."""
    if which == 'fpu':
        return bool(b.native_kind(i))
    m = i.mnemonic
    if which == 'string':
        return m in ('stosb', 'stosw', 'stosd', 'movsb', 'movsw', 'movsd', 'lodsb', 'lodsd', 'scasb', 'cmpsb')             or (m in ('rep', 'repne', 'repe') or i.prefix[0] in (0xf2, 0xf3)) and 'xmm' not in i.op_str
    if which == 'sse':
        xmm = any(op.type == 1 and i.reg_name(op.reg).startswith('xmm') for op in i.operands)
        return (xmm or m in ('ldmxcsr', 'stmxcsr', 'movntps')) and not b.native_kind(i)             and not any(op.type == 3 and op.mem.segment for op in i.operands)
    if b.native_kind(i) or m in EXCLUDED_INT or m in b.THREEDNOW or m.startswith('j'):
        return False
    if any(p not in (0, 0x66) for p in i.prefix):
        return False
    for op in i.operands:
        if op.type == 1 and i.reg_name(op.reg).startswith(('xmm', 'mm')):
            return False
        if op.type == 1 and i.reg_name(op.reg) in ('esp', 'sp'):
            return False          # the harness's stack pointer differs between the two runs
        if op.type == 3 and op.mem.segment:
            return False
    regs_read, regs_write = i.regs_access()
    written = {i.reg_name(r) for r in regs_write}
    read = {i.reg_name(r) for r in regs_read}
    if written & {'esp', 'sp'}:
        return False          # the harness's own stack
    return 'esp' not in read or any(op.type == 3 for op in i.operands)


def forms(b, which='fpu'):
    """One representative instruction per (mnemonic, operand kinds) used by the game."""
    data = (ROOT / 'local/game/Porsche.exe').read_bytes()
    if hashlib.sha256(data).hexdigest() != ORIGINAL_SHA:
        raise ValueError('Original Porsche.exe SHA mismatch')
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    md.detail = True
    seen = {}
    for line in CATALOG.read_text(encoding='utf-8').splitlines():
        e = json.loads(line)
        for lo, hi in e['ranges']:
            lo, hi = int(lo, 16), int(hi, 16) + 1
            for i in md.disasm(data[lo - 0x400000:hi - 0x400000], lo):
                if not wanted(b, i, which):
                    continue
                kinds = []
                for op in i.operands:
                    if op.type == 1:
                        r = i.reg_name(op.reg)
                        kinds.append(r if (r.startswith(('st', 'mm', 'xmm')) or r in b.GPR32 or r == 'ax') else 'reg')
                    elif op.type == 3:
                        kinds.append(f'm{op.size}')
                    else:
                        kinds.append('imm')
                key = (i.mnemonic, tuple(kinds))
                seen.setdefault(key, bytes(i.bytes))
    return seen


def reencode(raw, mode):
    """Memory operand -> [esi] (mode 0) or [esi+edi*2+0x10] (mode 1); same opcode/reg/imm."""
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    md.detail = True
    i = next(md.disasm(raw, 0))
    if not any(op.type == 3 for op in i.operands):
        return raw if mode == 0 else None
    k = i.modrm_offset
    if not k:
        return None          # moffs forms (mov eax, [abs]) have no ModRM to redirect
    reg = raw[k] & 0x38
    tail = raw[i.imm_offset:] if i.imm_offset else b''
    if mode == 0:
        return raw[:k] + bytes([reg | 0x06]) + tail
    return raw[:k] + bytes([0x80 | reg | 0x04, 0x5e]) + (0x10).to_bytes(4, 'little') + tail   # [esi+ebx*2+0x10]


def defined_flags(i):
    """Architecturally defined/preserved flags; never generalize Intel-only undefined bits."""
    m = i.mnemonic
    if m in ('div', 'idiv'):
        return '0x400u'  # Arithmetic flags undefined; DF preserved.
    if m in ('mul', 'imul'):
        return '0xc01u'  # Only CF/OF defined, DF preserved.
    if m in ('and', 'or', 'xor', 'test'):
        return '0xcc5u'  # AF undefined.
    if m in ('bsf', 'bsr'):
        return '0x440u'  # ZF defined; arithmetic flags otherwise undefined.
    if m in ('bt', 'bts'):
        return '0x441u'  # CF defined, ZF/DF preserved.
    if m in ('shl', 'sal', 'shr', 'sar', 'shld', 'shrd', 'rol', 'ror', 'rcl', 'rcr'):
        op = i.operands[-1]
        count = str(op.imm) + 'u' if op.type == 2 else 'r[1]'
        n = f'(({count}) & 31u)'
        bits = i.operands[0].size * 8
        effective = f'({n} % {bits + 1}u)' if m in ('rcl', 'rcr') else n
        mask = f'(0xcd5u & ~({n} > 1u ? 0x800u : 0u))'
        if m in ('shl', 'sal', 'shr', 'sar', 'shld', 'shrd'):
            mask = f'({mask} & ~0x10u)'
            if m in ('shl', 'sal', 'shr'):
                mask = f'({mask} & ~({n} >= {bits}u ? 1u : 0u))'
        return f'({effective} == 0u ? 0xcd5u : {mask})'
    return '0xcd5u'


HARNESS = r'''
#include <windows.h>
#include <stdio.h>
#include <string.h>
#include "machine257.h"
#define INTSET %INTSET%
#define STRINGSET %STRINGSET%
void dispatch257(Machine257& s, U target) { s.stop = Stop257::BadReturn; (void)target; }
%DECLS%
typedef void (*Lowered)(Machine257&);
typedef U (*FlagMask)(const U*);
struct Frag { const char* name; const unsigned char* code; U len; Lowered low; FlagMask mask; };
%FRAGS%
static unsigned long long rng = 0x9e3779b97f4a7c15ull;
static U rnd() { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; return (U)(rng >> 16); }
static void put80(unsigned char* p, int kind) {
    unsigned long long man = ((unsigned long long)rnd() << 32) | rnd(); unsigned exp = rnd() & 0x7fff; unsigned sign = rnd() & 1;
    switch (kind) {
    case 0: man |= 1ull << 63; exp = 0x3fff - 40 + (rnd() % 80); break;            // normal, moderate
    case 1: man |= 1ull << 63; exp = 1 + (rnd() % 0x7ffd); break;                   // normal, any exponent
    case 2: man = 0; exp = 0; break;                                                // zero
    case 3: man = 1ull << 63; exp = 0x7fff; break;                                  // infinity
    case 4: man |= 3ull << 62; exp = 0x7fff; break;                                  // quiet NaN
    case 5: man &= ~(1ull << 63); if (!man) man = 1; exp = 0; break;                 // denormal
    case 6: man = (unsigned long long)(rnd() % 1000) << 53 | (1ull << 63); exp = 0x3fff + 9; break;  // small integer-ish
    }
    memcpy(p, &man, 8); unsigned short se = (unsigned short)((sign << 15) | exp); memcpy(p + 8, &se, 2);
}
static int kind_tag(int kind) { return kind == 2 ? 1 : (kind == 0 || kind == 1 || kind == 6) ? 0 : 2; }
static const unsigned short CW[] = {0x037f, 0x027f, 0x007f, 0x0c7f, 0x047f, 0x087f, 0x0f7f, 0x017f};
static void image(unsigned char* img) {
    memset(img, 0, 108);
    unsigned top = rnd() & 7; unsigned short tw = 0;
    for (int r = 0; r < 8; ++r) {                       // r = physical register
        int empty = (rnd() % 5) == 0; int kind = rnd() % 7;
        unsigned tag = empty ? 3 : kind_tag(kind);
        tw |= (unsigned short)(tag << (2 * r));
        int st = (r - (int)top) & 7;                     // FNSAVE stores ST(i) in order
        put80(img + 28 + 10 * st, kind);
    }
    unsigned short cw = CW[rnd() % 8]; unsigned short sw = (unsigned short)((top << 11) | (rnd() & 0x4700) | (rnd() & 0x3f & 0));
    memcpy(img + 0, &cw, 2); memcpy(img + 4, &sw, 2); memcpy(img + 8, &tw, 2);
}
alignas(16) static unsigned char fpu_in[108], fpu_n[108], fpu_l[108];
alignas(16) static U xmm_in[8][4], xmm_n[8][4], xmm_l[8][4];
static int r_in_fix;
static U r_in[8], r_n[8], flags_in, flags_n, mx_in, mx_n, mx_l, saved_esp, code_ptr;
alignas(16) static unsigned char mem_in[512], buf[512], mem_n[512];
__declspec(naked) static void run_native() {
    __asm {
        pushad
        mov saved_esp, esp
        frstor fpu_in
        movups xmm0, xmm_in[0]
        movups xmm1, xmm_in[16]
        movups xmm2, xmm_in[32]
        movups xmm3, xmm_in[48]
        movups xmm4, xmm_in[64]
        movups xmm5, xmm_in[80]
        movups xmm6, xmm_in[96]
        movups xmm7, xmm_in[112]
        ldmxcsr mx_in
        pushfd
        pop eax
        and eax, 0xfffff72a
        or eax, flags_in
        push eax
        popfd
        mov eax, r_in[0]
        mov ecx, r_in[4]
        mov edx, r_in[8]
        mov ebx, r_in[12]
        mov ebp, r_in[20]
        mov esi, r_in[24]
        mov edi, r_in[28]
        call code_ptr
        mov r_n[0], eax
        mov r_n[4], ecx
        mov r_n[8], edx
        mov r_n[12], ebx
        mov r_n[20], ebp
        mov r_n[24], esi
        mov r_n[28], edi
        pushfd
        pop flags_n
        cld
        fnsave fpu_n
        stmxcsr mx_n
        movups xmm_n[0], xmm0
        movups xmm_n[16], xmm1
        movups xmm_n[32], xmm2
        movups xmm_n[48], xmm3
        movups xmm_n[64], xmm4
        movups xmm_n[80], xmm5
        movups xmm_n[96], xmm6
        movups xmm_n[112], xmm7
        mov esp, saved_esp
        popad
        ret
    }
}
static int same_fpu(const unsigned char* a, const unsigned char* b) {
    return !memcmp(a, b, 12) && !memcmp(a + 20, b + 20, 88);
}
%EDGEFUNC%
int main(int argc, char** argv) {
    int cases = argc > 1 ? atoi(argv[1]) : 200;
    int failed_forms = 0, total = 0;
    unsigned char* exec = (unsigned char*)VirtualAlloc(0, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    U fake_stack[64];
    for (U f = 0; f < sizeof(frags) / sizeof(frags[0]); ++f) {
        int bad = 0, skipped = 0, completed = 0; char why[160] = "";
        fprintf(stderr, "%s\n", frags[f].name); fflush(stderr);
        memcpy(exec, frags[f].code, frags[f].len); FlushInstructionCache(GetCurrentProcess(), exec, 4096);
        code_ptr = (U)exec;
        for (int c = 0; c < cases; ++c) {
            image(fpu_in);
            for (int k = 0; k < 512; ++k) mem_in[k] = (unsigned char)rnd();
            image(mem_in + 64);                               // a valid image at [esi]
            if (strstr(frags[f].name, "ebx*2")) { r_in_fix = 1; image(mem_in + 64 + 0x10); }  // and at [esi+ebx*2+0x10], ebx = 0
            else r_in_fix = 0;
            for (int r = 0; r < 8; ++r) r_in[r] = rnd();
            r_in[6] = (U)(buf + 64); r_in[3] = r_in_fix ? 0u : rnd() % 64; if (INTSET) r_in[7] = (U)(buf + 256);
            if (!strncmp(frags[f].name, "bt", 2)) r_in[0] = rnd() % 1024;   // bit strings stay inside the buffer
            if (STRINGSET) { r_in[1] = rnd() % 16; r_in[6] = (U)(buf + 192); r_in[7] = (U)(buf + 320); }
            // Nonfaulting DIV/IDIV domain. Fault semantics are not modeled by this backend.
            bool division = !strncmp(frags[f].name, "div ", 4) || !strncmp(frags[f].name, "idiv ", 5);
            if (division) {
                r_in[2] = 0; r_in[0] = rnd() & 0x7fu;
                if (!strcmp(frags[f].name, "idiv edx")) {
                    r_in[2] = 0xffffffffu; r_in[0] = 0xffff0000u | (rnd() & 0xffffu);
                }
            }
            flags_in = rnd() & 0x8d5;
            if (STRINGSET) flags_in |= (rnd() & 1u) << 10;         // DF
            for (int r = 0; r < 8; ++r) for (int l = 0; l < 4; ++l) xmm_in[r][l] = rnd();
            mx_in = 0x1f80 | ((rnd() & 3) << 13) | ((rnd() & 1) << 15);
            memcpy(buf, mem_in, 512);
            int nf = 0; __try { run_native(); } __except (1) { nf = 1; }
            if (nf) {
                ++skipped;
                if (!division || skipped > 100000) { strcpy(why, "unexpected native fault or exhausted input domain"); ++bad; break; }
                --c; continue;
            }
            memcpy(mem_n, buf, 512);
            memcpy(buf, mem_in, 512);
            Machine257 s; memset(&s, 0, sizeof(s));
            s.budget = 1000u;
            for (int r = 0; r < 8; ++r) s.r[r] = r_in[r];
            fake_stack[32] = 0x12345678u; s.r[4] = (U)&fake_stack[32];
            s.flags = flags_in; s.mxcsr = mx_in; memcpy(s.xmm, xmm_in, sizeof(xmm_in));
            __asm frstor fpu_in
            int lf = 0; __try { frags[f].low(s); } __except (1) { lf = 1; }
            __asm fnsave fpu_l
            __asm fninit
            if (lf || s.stop != Stop257::Returned) {
                strcpy(why, lf ? "lowered faulted" : "unexpected lowered stop"); ++bad; break;
            }
            const char* d = 0;
            for (int r = 0; r < 8; ++r) if (r != 4 && s.r[r] != r_n[r]) d = "gpr";
            U flag_mask = frags[f].mask(r_in);
            if ((s.flags & flag_mask) != (flags_n & flag_mask)) d = "eflags";
            if (memcmp(buf, mem_n, 512)) d = "memory";
            if (!same_fpu(fpu_l, fpu_n)) d = "x87 state";
            if (memcmp(s.xmm, xmm_n, sizeof(xmm_n))) d = "xmm";
            if (s.mxcsr != mx_n) d = "mxcsr";
            ++total;
            ++completed;
            if (d) { if (!bad) sprintf(why, "%s (case %d) flags native %03x lowered %03x in %03x", d, c, flags_n & 0x8d5u, s.flags & 0x8d5u, flags_in); ++bad; }
        }
        bool fail = bad || completed != cases;
        printf("%s\t%s\t%d/%d\t%d\t%d\t%d\n", fail ? "FAIL" : "PASS", frags[f].name, completed > bad ? completed - bad : 0, cases, completed, skipped, bad);
        if (fail) { ++failed_forms; printf("  first difference: %s\n", why); }
    }
    int edge_bad = %EDGECALL%;
    VirtualFree(exec, 0, MEM_RELEASE);
    printf("forms %u failed %d cases %d\n", (U)(sizeof(frags) / sizeof(frags[0])), failed_forms, total);
    return failed_forms != 0 || edge_bad != 0;
}
'''

DIV8_EDGES = r'''
static int div8_edges(unsigned char* exec) {
    const unsigned char code[] = {0xf6, 0xf3, 0xc3}; // DIV BL; RET
    const U ax[] = {5, 0xff, 0xfeff, 0x100, 0xffff, 0xffff};
    const U bl[] = {2, 1, 0xff, 1, 0, 0xff};
    int bad = 0;
    memcpy(exec, code, sizeof(code)); FlushInstructionCache(GetCurrentProcess(), exec, 4096);
    code_ptr = (U)exec;
    for (U k = 0; k < 6; ++k) {
        image(fpu_in); mx_in = 0x1f80; flags_in = 0x8d5;
        for (U r = 0; r < 8; ++r) r_in[r] = 0x12345600u + r;
        r_in[0] = 0xabcd0000u | ax[k]; r_in[3] = 0x87650000u | bl[k];
        bool fault_expected = !bl[k] || ax[k] / bl[k] > 0xffu;
        int native_fault = 0; __try { run_native(); } __except (1) { native_fault = 1; }
        Machine257 s = {}; U stack[4] = {}; s.budget = 100;
        for (U r = 0; r < 8; ++r) s.r[r] = r_in[r];
        s.r[4] = (U)&stack[1]; U old_sp = s.r[4]; s.flags = flags_in;
        translated_20000000(s);
        bool ok = bool(native_fault) == fault_expected;
        if (fault_expected) {
            ok = ok && s.stop == Stop257::UnsupportedInput && s.flags == flags_in;
            for (U r = 0; r < 8; ++r) ok = ok && s.r[r] == (r == 4 ? old_sp : r_in[r]);
        } else {
            ok = ok && s.stop == Stop257::Returned && s.r[0] == r_n[0];
            for (U r = 1; r < 8; ++r) if (r != 4) ok = ok && s.r[r] == r_n[r];
        }
        if (!ok) ++bad;
    }
    printf("div8_edges 6 failed %d\n", bad);
    return bad;
}
'''


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--cases', type=int, default=300)
    ap.add_argument('--set', choices=('int', 'sse', 'branch', 'string'), default='int')
    ap.add_argument('--mutate', choices=sorted(MUTANTS), help='negative control: a deliberately wrong lowering')
    ap.add_argument('--flags', choices=('defined', 'cpu-profile'), default='defined',
                    help='cpu-profile also compares undefined bits; results are CPU-specific')
    ap.add_argument('--vc-root', type=Path, help='MSVC version directory containing bin/Hostx64/x86/cl.exe')
    ap.add_argument('--output-dir', type=Path, help='fresh attempt directory; committed receipts are never overwritten')
    args = ap.parse_args()
    if args.cases < 1:
        ap.error('--cases must be positive')
    global WORK
    WORK = (args.output_dir or WORK / datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')).resolve()
    if WORK.exists():
        ap.error(f'output directory already exists: {WORK}')
    b = load_build()
    if args.set == 'branch':
        # jcc rel8 over 'mov eax,1; ret' to 'mov eax,2': every condition code 70..7f.
        form_list = {(f'jcc{op:02x}', ()): bytes([op, 0x06]) + bytes.fromhex('b801000000c3b802000000')
                     for op in range(0x70, 0x80)}
    else:
        form_list = forms(b, args.set)
    translate, header = b.backend()
    # Newly rejected widths must remain rejected, independent of catalog coverage.
    rejections = ['660fa30ec3', '660fab0ec3', 'f0fe06c3', 'f066ff0ec3', '660fa5c8c3', '660fadc8c3']
    for raw in rejections:
        try:
            translate(bytes.fromhex(raw), 0x30000000)
        except ValueError as e:
            if 'Unsupported' not in str(e):
                raise
        else:
            raise ValueError(f'unproved operand width was accepted: {raw}')
    if args.mutate == 'no-df':
        header = header.replace('(s.flags & 0x400u) ?', '(0u) ?')
    if args.mutate == 'rot-of':
        header = header.replace('U of = (imm && n > 1u) ?', 'U of = (1u) ?')
    if args.mutate == 'no-parity':
        header = header.replace('return (~v&1u)<<2;', 'return 0u*(~v&1u);')
    if args.mutate == 'no-af':
        header = header.replace('&0x10u', '&0u').replace('& 0x10u', '& 0u')
    WORK.mkdir(parents=True, exist_ok=True)
    (WORK / 'machine257.h').write_text(header, encoding='utf-8')
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_32)
    decls, frags, bodies, names, untranslated, excluded = [], [], [], [], [], []
    md.detail = True
    for (m, kinds), raw in sorted(form_list.items()):
        for mode in (0, 1):
            code = reencode(raw, mode) if args.set not in ('branch', 'string') else (raw if mode == 0 else None)
            if code is None:
                continue
            decoded = list(md.disasm(code, 0))
            if args.set not in ('branch', 'string') and (len(decoded) != 1 or decoded[0].size != len(code)):
                untranslated.append(f'{m} {kinds}: re-encoding not decodable')
                continue
            frag = code + b'\xc3'
            # DIV EDX cannot succeed: EDX is both divisor and dividend high word.
            # This verifier tests nonfaulting inputs, not the backend's inherited #DE gap.
            if decoded[0].mnemonic == 'div' and decoded[0].op_str == 'edx':
                excluded.append(dict(form='div edx', raw=code.hex(), reason='no nonfaulting input; #DE not modeled'))
                continue
            va = 0x10000000 + 0x100 * len(frags)
            try:
                low, _ = translate(frag, va)
            except ValueError as e:
                untranslated.append(f'{m} {kinds}: {e}')
                continue
            low = low.replace('#include "machine257.h"\n', '')
            text = decoded[0]
            name = f'{text.mnemonic} {text.op_str}'.replace('"', "'")
            bodies.append(low)
            decls.append(f'void translated_{va:08x}(Machine257&s);')
            mask = '0xcd5u' if args.flags == 'cpu-profile' else defined_flags(decoded[0])
            decls.append(f'static U flags_{va:08x}(const U* r){{return {mask};}}')
            arr = ', '.join(f'0x{x:02x}' for x in frag)
            frags.append(f'{{"{name}", (const unsigned char[]){{{arr}}}, {len(frag)}u, translated_{va:08x}, flags_{va:08x}}}')
            names.append(name)
    frag_arrays = []
    for k, f in enumerate(frags):
        arr = f.split('(const unsigned char[]){', 1)[1].split('}', 1)[0]
        frag_arrays.append(f'static const unsigned char code{k}[] = {{{arr}}};')
    table = 'static const Frag frags[] = {\n' + ',\n'.join(
        f.split('(const unsigned char[])', 1)[0] + f'code{k}, ' + f.split('}, ', 1)[1] for k, f in enumerate(frags)) + '\n};'
    if args.set == 'int':
        edge, _ = translate(bytes.fromhex('f6f3c3'), 0x20000000)
        bodies.append(edge.replace('#include "machine257.h"\n', ''))
        decls.append('void translated_20000000(Machine257&);')
    src = HARNESS.replace('%STRINGSET%', '1' if args.set == 'string' else '0').replace('%INTSET%', '1' if args.set in ('int', 'string') else '0').replace('%DECLS%', '\n'.join(decls)).replace('%FRAGS%', '\n'.join(frag_arrays) + '\n' + table)
    src = src.replace('%EDGEFUNC%', DIV8_EDGES if args.set == 'int' else '').replace('%EDGECALL%', 'div8_edges(exec)' if args.set == 'int' else '0')
    (WORK / 'harness.cpp').write_text(src, encoding='utf-8')
    (WORK / 'fragments.cpp').write_text('#include "machine257.h"\n' + '\n'.join(bodies), encoding='utf-8')
    if not frags:
        raise SystemExit('No testable fragments; refusing an empty PASS')
    cl, link, env = b.toolchain(vc_root=args.vc_root)
    flags = ['/nologo', '/TP', '/std:c++17', '/O2', '/GS-', '/EHs-c-', '/W0', f'/I{WORK}']
    command = [str(cl), *flags, 'harness.cpp', 'fragments.cpp', '/Fe:oracle.exe', '/link', 'user32.lib']
    r = subprocess.run(command, cwd=WORK, capture_output=True, text=True, env=env, timeout=180)
    (WORK / 'compile.log').write_text(r.stdout + r.stderr, encoding='utf-8', newline='\n')
    if r.returncode:
        print(r.stdout[-4000:], r.stderr[-2000:])
        raise SystemExit('compile failed')
    run_command = [str(WORK / 'oracle.exe'), str(args.cases)]
    run = subprocess.run(run_command, cwd=WORK, capture_output=True, text=True, timeout=180)
    (WORK / 'oracle.log').write_text(run.stdout + run.stderr, encoding='utf-8')
    lines = run.stdout.splitlines()
    results = [dict(form=p[1], result=p[0], cases=p[2], completed=int(p[3]), native_rejected=int(p[4]), differences=int(p[5]))
               for l in lines if len(p := l.split('\t')) == 6]
    total = sum(r['completed'] for r in results)
    expected = len(frags) * args.cases
    tail = re.fullmatch(r'forms (\d+) failed (\d+) cases (\d+)', lines[-1]) if lines else None
    edge_ok = args.set != 'int' or 'div8_edges 6 failed 0' in lines
    complete = (edge_ok and len(results) == len(frags) and [r['form'] for r in results] == names and total == expected and
                all(r['completed'] == args.cases for r in results) and tail is not None and
                tuple(map(int, tail.groups())) == (len(frags), sum(r['result'] != 'PASS' for r in results), total))
    summary = dict(schema=2, cpu=subprocess.run(['powershell', '-NoProfile', '-Command',
                                                 '(Get-CimInstance Win32_Processor | Select-Object -First 1).Name'],
                                                capture_output=True, text=True, timeout=20).stdout.strip(),
                   forms=len(results), failed=[r for r in results if r['result'] != 'PASS'],
                   cases_per_form=args.cases, completed=total, expected=expected, complete=complete,
                   div8_edges=6 if args.set == 'int' else 0, rejected_widths=rejections,
                   flag_policy=args.flags, mutant=args.mutate, exit_code=run.returncode,
                   untranslated=untranslated, excluded=excluded, tail=lines[-1] if lines else '')
    ok = complete and not untranslated and run.returncode == 0 and not summary['failed']
    if args.mutate:
        # A mutation check succeeds only on a complete run with actual mismatches, never on a crash.
        ok = complete and not untranslated and run.returncode == 1 and bool(summary['failed']) and any(r['differences'] for r in results)
        summary['mutation_detected'] = ok
    pin_paths = [Path(__file__), ROOT / 'scripts/research/x86-cpp-backend-exact.py',
                 ROOT / 'scripts/research/x86-cpp-backend-fpu.py', ROOT / 'scripts/research/x86-cpp-backend-277.py',
                 ROOT / 'scripts/research/x86-cpp-backend-257.py', ROOT / 'local/game/Porsche.exe', CATALOG,
                 cl, link, cl.parent / 'c1xx.dll', cl.parent / 'c2.dll',
                 *(WORK / p for p in ('machine257.h', 'harness.cpp', 'fragments.cpp', 'oracle.exe', 'oracle.log', 'compile.log'))]
    summary['sha256'] = {str(p): digest(p) for p in pin_paths}
    summary['commands'] = [command, run_command]
    summary['compiler_environment'] = {k: env[k] for k in ('INCLUDE', 'LIB')}
    summary['accepted'] = ok
    (WORK / 'receipt.json').write_text(json.dumps(dict(summary, results=results), indent=1) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps({k: summary[k] for k in ('cpu', 'forms', 'completed', 'expected', 'complete', 'flag_policy', 'mutant', 'exit_code', 'accepted')}, indent=1))
    for f in summary['failed'][:40]:
        print('FAIL', f)
    raise SystemExit(0 if ok else 1)


if __name__ == '__main__':
    main()
