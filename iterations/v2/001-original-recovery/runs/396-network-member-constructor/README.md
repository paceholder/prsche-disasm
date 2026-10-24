# Run396: original network member constructor004902c0

Final native001 passes1296 cases at each of `/Od` and `/O2` (2592 total),5184
checks per build, seven detected mutations. The complete81B body is recovered;
source/config/fixture are frozen for a later integration, separate from392.

Original `Porsche.exe` SHA256
`ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`,
`004902c0..00490310`,81B including `RET4`. Index queries cover constructor,
caller0048fcd9,005e4fe8, default/live managers,0059eeb0 and lock helpers.
Raw assembly is complete and unambiguous;391 already retained its full body
when establishing the opaque argument contract. No new REA session or lowering
experiment was needed. `evidence.json` records exact raw/data/source pins.

The original loads current manager before entering its lock and captures
manager+28 once. It checks pool+10 after entering, invokes canonical refill
`0059eeb0(1,0)` when empty, reloads/populates the head, reloads the captured
pool lock for leave, then stores receiver/node self-links. It reloads receiver
after writing node[0], preserving even overlapping byte aliases, and returns
receiver in EAX. The stack pointer argument is ignored: the raw81B body never
reads it and ends with RET4. C++ uses the established fastcall binding with
unused EDX and one opaque stack pointer.

There are no new mutable cells or allocator implementations. Canonical
`application_context_heap_state_005e4fe8()` reads the physical
`application_heap_contexts.current` owned by `application_heap_state.cpp`.
Canonical bytewise address/load/store helpers preserve accesses without a new
manager layout. Refill `application_context_new_head_0059eeb0` already belongs
to `context_ranges.cpp`; its exact registry entry and prior153 report are
recorded. Heap enter/leave are existing typed callees.

The fixture links the actual canonical state owner, whose source hash equals
accepted340/025. Raw current/default/live headers, actual current/selectors and
zero record/runtime defaults are asserted before reset or relocation. Original
A1 points to the canonical physical current cell; C++ calls the actual getter.
All81 raw bytes, three CALL targets and the original A1/RET4 are checked.
Unrelated owner callbacks/shutdown paths fail closed. Lock/refill endpoints
record controlled child effects; they do not enter the production graph.
Own COFF proves the constructor definition, canonical unresolved calls, no
duplicate state, and the actual owner/getter definitions in the linked owner TU.

Cases use default/live managers, empty/nonempty head, six node placements,
receiver/node/pool/current aliases and mapped1..3-byte overlaps. Enter/refill/
leave can switch the global manager or replace captured lock/head. Snapshots
compare event order/arguments, callback-visible state, the complete actual
contexts/runtime and all256KiB of patterned arena memory. PAGE_NOACCESS,
NULL,0xffffffff and mapped opaque arguments prove ignored transport. ESP,
EBP/EBX/ESI/EDI, EAXself and callee RET4 are checked. Mutations detect manager
recapture, wrong refill kind, next-word offset, lock offset, missing first
self-link, missing receiver reload and incorrect return pointer.

Actual allocator/lock/OS composition and full startup/game/visual proof remain
open. Receiver/node access must be mapped; original refill failure has no null
guard and no invented failure return is added. Concurrent writes and arbitrary
unmapped/malformed aliases remain HOLD.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/396-network-member-constructor/native-config.json --attempt 1
```

Receipt: `local/experiments/v2-network-member-constructor-native-396/001/receipt.json`.
All inputs still match. No registry/root-plan/CMake/resource edits or commits.
