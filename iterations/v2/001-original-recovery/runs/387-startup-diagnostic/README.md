# Run387: original startup diagnostic varargs callback

Native001 **PASS**: 672 comparisons / 2016 checks in each `/Od` and `/O2`,
with five detected mutations. New original entry `0059fbc0..0059fc1c`,93B.
No new mutable state. Root owns registry/game intake and later startup pointer
binding; this batch does not change the frozen dispatch owner.

Original `Porsche.exe` SHA256:
`ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`.
The omitted entry is now navigation in uniquely named
`research/binary-index/supplemental/startup-diagnostic-387.jsonl`. Complete raw
bytes/instructions, final RET, address-taking store004a5424 and three calls are
pinned. Its93 bytes also match existing raw assembly through0059fc1c. No accepted
entry/body overlaps it. `evidence.json` retains raw/source/state/consumer details.

The callback forwards cdecl varargs immediately after the format word to actual
353 `application_diagnostic_mono_vsprintf_005a299e`. It then reads canonical
line005deb78 and file005deb74 **after** that formatter returns, scans the output
and appends the exact raw20B context format ` [FILE=%s, LINE=%d]` at its NUL.
Actual canonical `formatter_entry_005a0fbf` performs the second call. Shared
state remains owned by `shared_runtime_globals.cpp`; no pointer mapping or
state reset is introduced. Both formatter return values are ignored, including
negative values. Public callback/terminal contracts are void, matching existing
diagnostic consumers; no meaningful EAX status or forced success is invented.

The fixture links actual353 vsprintf, canonical formatter entry and actual
shared state owner. The copied original93B body calls those same accepted
providers and a controlled cdecl terminal endpoint. Formatter core output is
a synthetic typed transport seam; parser/cleanup algorithms retain their existing
proof rather than using host printf. Unused mono/cleanup paths fail closed.
Raw context bytes and production shared defaults/aliases are checked before
resets or relocation. Own callback address/source-state COFF evidence is pinned
in `coff-evidence.json`, including the unresolved terminal provider.

Cases compare eight raw vararg words, both formatter descriptor prefixes,
arguments/format bytes, output offsets, combined text, final canonical state,
ESP and EBP/EBX/ESI/EDI. First formatter effects replace file/line; second-call
effects can replace them again. Output spans include empty/short/long messages
and exactly1023 combined bytes. Zero/negative formatter returns, high line bits,
NULL and opaque raw file tokens are transported unchanged. Opaque file tokens
are only recorded by the controlled core; they do **not** establish valid host
strings. Mutations detect advanced varargs, swapped context arguments, early
file/line capture, wrong append base and omitted terminal call.

The original1024B local has no1024-byte formatter limit: its reused wrapper
sets `INT_MAX`. Combined output beyond1023 or missing termination remains HOLD;
no clamp/guard replaces original overflow behavior. Real parser/raw-original-VA
string resolution, arbitrary aliases, concurrent state changes and actual
terminal/full-startup composition remain separate acceptance boundaries.

`0059fc20` stays an explicit `void __cdecl(const char*)` external contract,
borrowing the NUL-terminated text during the call. Its indexed356B body includes
missing-file callback, debug mode, thread/lock, MessageBox, dynamic debugger and
exit effects. It copies253 bytes then forces byte254 to zero, leaving byte253
opaque for input lengths>=253. Raw defaults are mode005e50b4=2 and debugger
cache005deb68=1. None of those effects/state/opaque bytes is approximated here.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/387-startup-diagnostic/native-config.json --attempt 1
```

Receipt: `local/experiments/v2-startup-diagnostic-native-387/001/receipt.json`.
Source/native inputs are frozen. No old source, shared index README, registry,
CMake or resource edits; no new runner or commit.
