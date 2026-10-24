# Run400: original debugger predicate and break gate

Final native002 passes16384 comparisons at each of `/Od` and `/O2` (32768
total),65536 checks per build and seven detected mutations. Both original and
C++ execute a real INT3, continue through a bounded native handler and return
with the required ABI. Source/config/fixture are frozen for coordinator intake.

Original `Porsche.exe` SHA256
`ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`:
`005af090..005af0b9`,42B predicate; `005af0c0..005af0d9`,26B gate. Existing
index contains both complete bodies; no accepted entry/body overlaps them.
Queries cover both functions,005de020,005b20a8, API strings and390 callers.
Raw assembly settles behavior, so no new REA session or lowering run was needed.
Unsupported lowering of INT3 is not replaced with a no-op.

`startup_debugger_query_005af090()` loads exact `kernel32.dll`, resolves exact
`IsDebuggerPresent`, invokes its stdcall pointer and returns the full signed
BOOL. Load/procedure failures return zero. Original module retention is kept;
there is no FreeLibrary or added cache. `startup_debug_break_005af0c0()` reads
canonical flag once: zero returns; nonzero calls the predicate. A nonzero result
executes real `__debugbreak()`/INT3 and returns if continued. Zero reaches the
real DebugBreak import, corresponding to original `JMP [005b20a8]`. DebugBreak
and INT3 remain distinct effects. The public gate contract is void, matching390;
no incidental EAX status is invented.

No mutable state is added.005de020 already belongs to
`window_event_flag_005de020` in `window_event_translation.cpp`, initialized to
zero, also read by005600e8. The actual owner TU is linked in the fixture; raw
and production defaults are asserted before resets.400 COFF references that
existing symbol and proves two new bodies plus the exact LoadLibraryA,
GetProcAddress and DebugBreak imports. Owner COFF proves the existing flag
definition. Compiled `/Od` and `/O2` disassembly both contain real INT3.

The oracle retains all68 original bytes, including CC and tail JMP. Each
literal/state/IAT operand and direct predicate call is checked before
relocation. Original gate calls original predicate; C++ gate uses its actual
C++ predicate. Exact APIs are intercepted through the fixture EXE's own IAT.
Unused consumers of the flag owner fail closed. A vectored handler accepts
only EXCEPTION_BREAKPOINT within the active gate, at an actual CC byte, with
EIP at CC or CC+1; it records the effect and resumes at CC+1. The original
instruction is never patched out. ESP and EBP/EBX/ESI/EDI are checked after
normal returns, including continued INT3; predicate EAX is compared exactly.

Cases cover zero/high/nonzero flags and module handles, missing/present
procedure and query results0/1/-1/INT_MIN. Each API independently leaves the
flag unchanged or changes it to0/1/0x80000000. Snapshots compare call order,
arguments, exact strings, callback-visible flag, final flag and predicate result.
This also proves the gate does not reconsider its original flag decision after
API effects. Mutations detect flag inversion, booleanized query return, wrong
module forwarding, wrong failure return, imported API replacing INT3, an added
flag check before fallback and an incorrect predicate comparison.

Live loader/debugger behavior, attached-debugger continuation choices, internal
call/exception stack layout, actual imported DebugBreak exception behavior and
full terminal/window/game composition remain outside this bounded fixture.
Computed aliases and concurrent flag writes remain HOLD. Import transport and
normal ABI do not claim byte-identical compiler output or live debugger proof.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/400-debug-runtime/native-config.json --attempt 2
```

Receipt: `local/experiments/v2-debug-runtime-native-400/002/receipt.json`.
Attempt001 baseline passed; a control deleting the only DebugBreak call was
rejected because it removed the required IAT entry.002 replaces that control
with a wrong flag reread while retaining the import; production was unchanged.
No old source, registry, plan, CMake or resource changes; no commits by this worker.
