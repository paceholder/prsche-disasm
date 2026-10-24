# Run405: original file delete and physical-view consumers

Native003 PASS:4068 comparisons per `/Od` and `/O2`,8136 total;16272 checks per
variant,six detected mutations. Frozen source recovers original00591980(20B) and
00592490(53B), with no new state or unresolved child provider.

00591980 clears last-error then forwards its borrowed filename token to exact
KERNEL32!DeleteFileA, retaining full BOOL return bits.00592490 complements its
encoded token, applies signed bounds against the existing physical-file table,
checks its active byte and returns viewDWORD at+0x14. Invalid inputs set error6;
valid lookup preserves last-error. Existing files.cpp owns count/pointer; the
existing PhysicalFile32B layout is asserted, not duplicated in game storage.

The shared fixture compares full original bodies, exact import operands/own-IAT
transport, returns, ordered callback-visible effects, full bounded memory and
cdecl ABI. Tokens cover invalid/NULL/high bits, tableNULL/countsigned boundaries,
slot0 and lastslot, inactive/nonzero activebytes, all view/result bits and state
replacement inside API boundaries. Controlled DeleteFileA performs no deletion.
Private fixture state is bounded transport; the integration audit must prove
actual canonical files.obj ownership and retain its source/header pins.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/405-file-backends/native-config.json --attempt 3
```

Receipt:local/experiments/v2-file-backends-native-405/003/receipt.json.
001/002 fail-closed fixture checks caught an IAT offset transcription and a reused
three-slot import-presence array respectively; source semantics were unchanged.
No bespoke runner/REA session/lowering census was needed for these complete raw
bodies. Real OS deletion, concurrent lifetime/TLS semantics, whole filesystem and
full-game acceptance remain open. Source/config/fixture are frozen at003.
