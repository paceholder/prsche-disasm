"""Run 431: hardware oracle for `lock inc/dec` in x86-cpp-backend-exact.py.

The game's only LOCK instructions are `lock inc/dec dword ptr [006c02e8]` (CRT locale reader
count, 005ae3c0/005ae490). The lowering (x86-cpp-backend-exact.py) uses
_InterlockedExchangeAdd. This oracle checks, on this CPU, with the operand as [esi]:
1. flags and memory: native vs lowered from the same deterministic values and EFLAGS (200000 cases,
   including 0x7fffffff/0x80000000/0xffffffff/0 edges);
2. atomicity: two threads run the lowered instruction 2,000,000 times each on one counter;
   the result must be exact. A negative control (the plain read-modify-write lowering) must
   lose updates.

Requires MSVC x86 and identity-mapped native addresses for Machine257 memory.
Usage: py -3 scripts/research/verify-v2-x86-exact-lock.py [--output-dir FRESH_DIR] [--vc-root PATH]
Receipts and build files stay in the fresh output directory; committed Run431 receipts are untouched.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / 'local/experiments/v2-x86-exact-lock'

HARNESS = r'''
#include <windows.h>
#include <stdio.h>
#include <string.h>
#include "machine257.h"
static_assert(sizeof(void*) == 4, "MSVC x86 identity-mapped addresses required");
void dispatch257(Machine257& s, U target) { s.stop = Stop257::BadReturn; (void)target; }
void translated_10000000(Machine257& s);   // lock inc dword ptr [esi]; ret
void translated_10000100(Machine257& s);   // lock dec dword ptr [esi]; ret
void plain_inc(Machine257& s);             // negative control: non-atomic read-modify-write
static unsigned long long rng = 88172645463325252ull;
static U rnd() { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; return (U)rng; }
static const unsigned char INC[] = {0xf0, 0xff, 0x06, 0xc3}, DEC[] = {0xf0, 0xff, 0x0e, 0xc3};
static U cell, flags_in, flags_n, code_ptr;
__declspec(naked) static void run_native() {
    __asm {
        push esi
        pushfd
        pop eax
        and eax, 0xfffff72a
        or eax, flags_in
        push eax
        popfd
        lea esi, cell
        call code_ptr
        pushfd
        pop flags_n
        pop esi
        ret
    }
}
static volatile U counter;
static DWORD WINAPI worker(void* p) {
    void (*fn)(Machine257&) = (void (*)(Machine257&))p;
    U stack[8]; Machine257 s; memset(&s, 0, sizeof(s)); s.budget = 1000u;
    for (int k = 0; k < 2000000; ++k) {
        stack[4] = 0; s.r[4] = (U)&stack[4]; s.r[6] = (U)&counter; s.stop = Stop257::Running; s.budget = 1000u;
        fn(s);
        if (s.stop != Stop257::Returned) return 1;
    }
    return 0;
}
int main() {
    unsigned char* exec = (unsigned char*)VirtualAlloc(0, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    if (!exec) { fprintf(stderr, "VirtualAlloc failed: %lu\n", GetLastError()); return 2; }
    int bad = 0;
    static const U edges[] = {0u, 1u, 0x7fffffffu, 0x80000000u, 0xffffffffu, 0x7ffffffeu, 0x80000001u};
    for (int op = 0; op < 2; ++op) {
        memcpy(exec, op ? DEC : INC, 4);
        if (!FlushInstructionCache(GetCurrentProcess(), exec, 4)) {
            fprintf(stderr, "FlushInstructionCache failed: %lu\n", GetLastError());
            VirtualFree(exec, 0, MEM_RELEASE); return 2;
        }
        code_ptr = (U)exec;
        for (int c = 0; c < 100000; ++c) {
            U v = c < 7 ? edges[c] : rnd(); flags_in = rnd() & 0x8d5u;
            cell = v; run_native(); U nv = cell, nf = flags_n;
            U stack[8]; stack[4] = 0; Machine257 s; memset(&s, 0, sizeof(s)); s.budget = 1000u;
            cell = v; s.r[6] = (U)&cell; s.r[4] = (U)&stack[4]; s.flags = flags_in;
            if (op) translated_10000100(s); else translated_10000000(s);
            if (s.stop != Stop257::Returned || cell != nv || (s.flags & 0x8d5u) != (nf & 0x8d5u)) {
                if (!bad) printf("diff op=%d v=%08x\n", op, v); ++bad;
            }
        }
    }
    printf("flags_memory_cases 200000 bad %d\n", bad);
    void* fns[2] = {(void*)translated_10000000, (void*)plain_inc};
    const char* names[2] = {"lowered", "plain"};
    U results[2];
    for (int k = 0; k < 2; ++k) {
        counter = 0;
        HANDLE t[2] = {0, 0};
        for (int j = 0; j < 2; ++j) {
            t[j] = CreateThread(0, 0, worker, fns[k], 0, 0);
            if (!t[j]) {
                fprintf(stderr, "CreateThread failed: %lu\n", GetLastError());
                if (t[0]) CloseHandle(t[0]);
                VirtualFree(exec, 0, MEM_RELEASE); return 2;
            }
        }
        DWORD wait = WaitForMultipleObjects(2, t, TRUE, 30000);
        if (wait != WAIT_OBJECT_0) {
            fprintf(stderr, "Thread wait failed: %lu error %lu\n", wait, GetLastError());
            CloseHandle(t[0]); CloseHandle(t[1]);
            VirtualFree(exec, 0, MEM_RELEASE); return 2;
        }
        bool threads_ok = true;
        for (int j = 0; j < 2; ++j) {
            DWORD exit_code = 1;
            if (!GetExitCodeThread(t[j], &exit_code) || exit_code != 0) threads_ok = false;
            if (!CloseHandle(t[j])) threads_ok = false;
        }
        if (!threads_ok) { fprintf(stderr, "Thread execution/cleanup failed\n"); VirtualFree(exec, 0, MEM_RELEASE); return 2; }
        results[k] = counter;
        printf("atomicity %s %u of 4000000\n", names[k], counter);
    }
    if (!VirtualFree(exec, 0, MEM_RELEASE)) { fprintf(stderr, "VirtualFree failed\n"); return 2; }
    int ok = bad == 0 && results[0] == 4000000u && results[1] < 4000000u;
    printf("%s\n", ok ? "PASS" : "FAIL");
    return !ok;
}
'''


def _sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def _check_output(output):
    """Require each complete check once; partial/ambiguous stdout cannot certify a run."""
    lines = output.splitlines()
    flags = [re.fullmatch(r'flags_memory_cases (\d+) bad (\d+)', line) for line in lines]
    flags = [match for match in flags if match]
    atomic = [re.fullmatch(r'atomicity (lowered|plain) (\d+) of (\d+)', line) for line in lines]
    atomic = [match for match in atomic if match]
    checks = {}
    if len(flags) != 1 or tuple(map(int, flags[0].groups())) != (200000, 0):
        raise ValueError('Missing/malformed/failed exact 200000-case flags check')
    checks['flags_memory_cases'] = 200000
    for name in ('lowered', 'plain'):
        found = [match for match in atomic if match[1] == name]
        if len(found) != 1 or int(found[0][3]) != 4000000:
            raise ValueError(f'Missing/malformed atomicity {name} check')
        checks[name] = int(found[0][2])
    if checks['lowered'] != 4000000 or not 0 <= checks['plain'] < 4000000:
        raise ValueError('Atomic result incorrect or negative plain control failed to lose updates')
    if lines.count('PASS') != 1 or not lines or lines[-1] != 'PASS' or 'FAIL' in lines:
        raise ValueError('Missing/ambiguous final PASS')
    # A malformed extra summary is also evidence of a broken harness/protocol.
    if sum(line.startswith('flags_memory_cases') for line in lines) != 1 or sum(line.startswith('atomicity') for line in lines) != 2:
        raise ValueError('Malformed/duplicate check summaries')
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, help='Fresh directory for build and schema2 receipt')
    parser.add_argument('--vc-root', type=Path, help='MSVC toolset root passed to the shared toolchain')
    args = parser.parse_args()
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    work = (args.output_dir if args.output_dir is not None else WORK / stamp).resolve()
    try:
        work.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error(f'Output directory must be fresh: {error}')
    receipt_path = work / 'lock-oracle.json'
    shared = ROOT / 'scripts/research/verify-v2-x86-exact.py'
    receipt = dict(schema=2, created_utc=stamp, passed=False, output_dir=str(work),
                   scope='MSVC x86, identity-mapped native memory; deterministic flags and contention proof',
                   invocation=[sys.executable, *sys.argv], vc_root=str(args.vc_root) if args.vc_root else None,
                   sources={}, generated={}, toolchain={}, executable={}, commands={}, checks={})

    def save():
        receipt_path.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')

    def run_stage(name, command, timeout, env):
        receipt['commands'][name] = command
        save()
        try:
            result = subprocess.run(command, cwd=work, capture_output=True, text=True, env=env, timeout=timeout)
        except subprocess.TimeoutExpired as error:
            def decoded(value):
                return value.decode(errors='replace') if isinstance(value, bytes) else value or ''
            receipt[name] = dict(timeout_seconds=timeout, timed_out=True, exit_code=None,
                                 stdout=decoded(error.stdout), stderr=decoded(error.stderr))
            raise RuntimeError(f'{name} timed out after {timeout}s') from error
        receipt[name] = dict(timeout_seconds=timeout, timed_out=False, exit_code=result.returncode,
                             stdout=result.stdout, stderr=result.stderr)
        if result.returncode:
            raise RuntimeError(f'{name} exited with {result.returncode}')
        return result

    try:
        receipt['sources'] = {str(path): _sha(path) for path in (Path(__file__).resolve(), shared)}
        receipt['sources'].update({str(ROOT / 'scripts/research' / name): _sha(ROOT / 'scripts/research' / name)
                                   for name in ('x86-cpp-backend-exact.py', 'x86-cpp-backend-fpu.py',
                                                'x86-cpp-backend-277.py', 'x86-cpp-backend-257.py')})
        spec = importlib.util.spec_from_file_location('oracle431', shared)
        o = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(o)
        b = o.Backend()
        translate, header = b.backend()
        (work / 'machine257.h').write_text(header, encoding='utf-8')
        inc, _ = translate(bytes.fromhex('f0ff06c3'), 0x10000000)
        dec, _ = translate(bytes.fromhex('f0ff0ec3'), 0x10000100)
        plain = ('void plain_inc(Machine257&s){ U a=read257(s.r[6],4u); write257(s.r[6],4u,a+1u); '
                 's.stop=Stop257::Returned; }\n')
        (work / 'fragments.cpp').write_text(inc + '\n' + dec.replace('#include "machine257.h"\n', '') + '\n' + plain,
                                           encoding='utf-8')
        (work / 'harness.cpp').write_text(HARNESS, encoding='utf-8')
        receipt['generated'] = {name: _sha(work / name) for name in ('machine257.h', 'fragments.cpp', 'harness.cpp')}
        cl, link, env = b.toolchain(vc_root=args.vc_root)
        receipt['toolchain'] = {name: dict(path=str(path), sha256=_sha(Path(path))) for name, path in (('compiler', cl), ('linker', link))}
        command = [str(cl), '/nologo', '/TP', '/std:c++17', '/O2', '/GS-', '/EHs-c-', '/W0', f'/I{work}',
                   'harness.cpp', 'fragments.cpp', '/Fe:lock.exe']
        run_stage('compile', command, 180, env)
        executable = work / 'lock.exe'
        receipt['executable'] = dict(path=str(executable), sha256=_sha(executable))
        if receipt['executable']['sha256'] is None:
            raise RuntimeError('Compiler returned success without lock.exe')
        run = run_stage('native', [str(executable)], 120, env)
        receipt['checks'] = _check_output(run.stdout)
        receipt['passed'] = True
        print(run.stdout, end='')
    except Exception as error:
        receipt['error'] = f'{type(error).__name__}: {error}'
        print(receipt['error'], file=sys.stderr)
    finally:
        executable = work / 'lock.exe'
        receipt['executable'] = dict(path=str(executable), sha256=_sha(executable))
        save()
    print(f'Receipt: {receipt_path}')
    return 0 if receipt['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
