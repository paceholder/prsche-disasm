# Run390: original diagnostic terminal0059fc20

Partial acceptance: the complete356B original body is recovered, with bounded
short-text/platform transport verified. Final native004 passes5202 comparisons
at each of `/Od` and `/O2` (10404 total),20808 checks per build and eight detected
mutations. Source/config/fixture are frozen. Integration is owned by Run392.

Original `Porsche.exe` SHA256
`ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`;
`0059fc20..0059fd83`,356B/bodySHA
`9b250b0431cf3681a5ab0b8d07b5889e3842d600e4027ef64182eacda92f2d01`.
Index queries cover terminal, copy, thread, break, mode/cache and existing
callback/lock owners. Full assembly and REA390/002 agree. Generated lowering390
was inspection material; original native bytes provide the behavior oracle.
`evidence.json` retains raw pins, owners, direct consumers, tests and boundaries;
`validation.json` is the existing shared five-query REA audit, preserved unchanged.

The terminal loads canonical missing-file callback once and calls `(text,-2)`
before copying or reading mode. Actual canonical253-byte copy is reused;
buffer254 is forced to zero and the raw newline appended. Mode3 writes debug
output then calls the explicit unknown break helper; mode1 writes output;
other modes except2 return. Mode2 calls canonical current-thread predicate.
Main-thread MessageBox response7 returns; other responses dynamically query
IsDebuggerPresent, reread the persistent cache, optionally output/break, then
call actual canonical process-exit with1. Non-main handling conditionally enters
the shared lock, displays MessageBox, reloads the lock and conditionally leaves.
The raw flags, exact titles/API names, negative returns and failure effects are
preserved. There is no FreeLibrary call in the original path.

Only new mutable cells are `startup_diagnostic_mode_005e50b4=2` and
`startup_debugger_present_005deb68=1`. Raw initial values are asserted before
fixture resets. Indexed absolute consumers are one mode read and cache
write/read in this body. Existing callback/lock storage remains in `files.cpp`
and `file_threads.cpp`;390 references those symbols. COFF proves both new cells
and terminal definition, canonical externs, unknown break extern and four exact
Win32 imports. Canonical game-owner COFF is checked separately during392.

The fixture retains all356 raw bytes and checks every patched operand before
relocation, including seven direct calls and seven IAT occurrences. Real Win32
imports in the production object are hooked through the fixture EXE's own IAT.
Actual formatter_strings/process_exit sources are linked. File/thread/lock
state and callees are controlled fixture endpoints with owner-source pins;
unused formatter-dialog endpoints fail closed. The exit wrapper reaches a
recording shutdown endpoint `(1,0,0)` and controlled terminal SEH. Fixture
endpoints do not enter the game graph. Normal returns verify ESP and
EBP/EBX/ESI/EDI; terminal paths compare effects and the exception edge.

Cases cover lengths0,1,31,200,251,252; modes0/1/2/3/4/-1/INT_MIN; callback
presence, text/mode edits and callback removal; main/non-main/negative predicate;
MessageBox0/6/7/-1; load/procedure failures; debugger0/1/-1 and cache effects;
lock changes by enter/MessageBox, including null/replaced lock. Snapshots compare
call order, strings, arguments, shared-state effects and terminal behavior.
Mutations detect callback reason, source pointer, newline, mode3, MessageBox
decision, debugger result, exit code and missing lock reload.

Input length>=253 remains HOLD: copy writes0..252, byte254 becomes zero,
byte253 remains uninitialized and scan-observable. The C++ local deliberately
retains that opaque byte; no zero fill/clamp or long-domain fidelity is claimed.
Concurrency, computed aliases, actual dialogs/debugger/locks/shutdown and full
startup/game/visual acceptance remain open. `startup_debug_break_005af0c0()` is
an unrecovered typed boundary: original26B code reads005de020, calls005af090,
and may INT3 or tail-jump an imported endpoint; it is not replaced with DebugBreak.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/390-diagnostic-terminal/native-config.json --attempt 4
```

Receipt: `local/experiments/v2-diagnostic-terminal-native-390/004/receipt.json`.
Attempts001/002 preserve an unused-TU link failure and rejected insensitive
copy-count mutation.003 passed but pinned the growing registry;004 removes
that pin before proof. No successful proof was repeated after final004.
No registry/CMake/root-plan edits, resource writes or commits by this worker.
