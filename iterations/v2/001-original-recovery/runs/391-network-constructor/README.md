# Run391: original startup network constructor

Recovers full Porsche.exe0048fc80..0048ff2d (686B) with its actual ECX receiver
and EAX=self return, plus an explicit cdecl adapter for the frozen startup
dispatcher. Sources preserve sparse fields, five sentinel allocations and
pointer reloads, early destructor/free order, two independent optional member
allocations, string copies, timer scaling and actual typed callback address.
Canonical providers/state are referenced directly; no new object layout,
vptr, raw-VA mapping or child stubs enter production.

Shared native003 **PASS**: **3456 cases per /Od and /O2**, each comparing the
complete original body against both the recovered entry and cdecl facade;
**17283 checks per variant, nine detected mutations**. Whole-object canaries,
child blocks, ordered pre-call state/arguments and stack/nonvolatile/EAX checks
cover early failure, short-circuit options, all small-allocation failures,
sentinel aliases into object fields, child writes, replaced child returns,
timer wraparound and shared strings of lengths0/19/31. New production initial
labels/timeout are checked before resets; external cells are controlled and
independently checked against raw PE data/zero-fill. Actual owner sources and
canonical proof inputs are pinned in the receipt.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/391-network-constructor/native-config.json --attempt 003
```

Passed sources/config/fixture are frozen. Authoritative receipt:
`local/experiments/v2-network-constructor-native-391/003/receipt.json` (242 pins).
Evidence records full raw bytes/index/caller, shared REA390/002 dossier hashes,
ABI and state owners. Exact-backend output is an inspection-only artifact at
`local/experiments/v2-network-constructor-native-391/inspection-candidate.cpp`;
its successful translation is not an extra game provider or behavioral proof.

New state005df6a8 starts at original400. Existing startup network628c70, timer
5deb48, missing-file callback6afbe4 and heap-name5e8e50 owners are reused.
The latter uses the existing32B application heap-name owner; historical
file_null_name/network_group_text aliases remain unresolved. Callback48d9b0
and six child bodies remain typed externs. Existing canonical children remain
controlled endpoints in this constructor proof; their earlier proofs are
reused separately. Original4902c0 ignores its opaque stackbyte argument,
confirmed through the full81B body/RET4; the fixture never reads that byte.

001 preserved a linkage experiment with whole external state-owner TUs that
exposed51 unrelated unresolved references.002 passed the native bodies and
eight mutations, then Windows rejected the final executable name containing
`install` with WinError740.003 changes only that mutation's label and passes
all nine. No accepted proof was rerun after cosmetic prose edits.

Five sentinel allocations must supply writable nonnull12B spans; original
NULL dereference is preserved. Receiver needs writable0x268B, and shared text
must terminate within32B. Real destructor, child algorithms, network/locks,
callback invocation, alias consolidation, concurrency/reinit and live game
startup remain open. Coordinator owns graph/registry integration in392.
