# Run386: bounded heap-startup caller binding

The existing `void __cdecl application_heap_finish(U,U,U)` facade now delegates
to canonical partial `application_pool_initialize_005679f0(U,U,I)`, discarding its
result. Coordinator approved this exact binding after native001. There are zero
new original VAs, algorithms or production state; neither005679f0 nor00591820 is
promoted from partial status. Passed canonical sources remain unchanged.

Original Porsche.exe SHA-256:
`ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`.
385's raw-order/ownership handoff is reused. The25 original bytes0059eaef..
0059eb07 load the third/fourth startup arguments, push fourth/zero/third, call
005679f0 and discard EAX. ADD ESP,0x3c cleans the new12 bytes plus48 prior pending
argument bytes; it does not imply15 callee arguments. The fixture labels its
synthetic stack prefix and compares the actual cleanup/ESI restore/RET.

Raw247-byte pool,149-byte disk initialization,74-byte page allocation,64-byte
list initialization,66-byte prepend and12-byte exit-registration bodies execute
as one native oracle. The C++ side links the four unchanged canonical component
files. Their old078 six-case proof/source hashes are reused. New386 proof covers
the concrete startup `(256,0,256)` path and caller composition, rather than
duplicating accepted algorithm implementations.

At page size4096, that call allocates8,192 disk-table bytes,4,096 device bytes
from the0xe00 request, and12,288 operation bytes. Exactly256 complete48-byte
nodes are prepended in descending address order. Four lock identities and the
shutdown callback registration occur in original order. Nearby counts prove
rounding controls list length:257 operations produce16,384 bytes and341 nodes.

Shared native001 PASS:44 cases per `/Od` and `/O2`,88 comparisons total; all four
binding mutations detected. Cases cover concrete cold/warm page-cache startup,
zero/default and asymmetric counts, rounding boundaries, callback/lock side
effects, and a second call after successful initialization. Complete3×64KiB
arenas,11 state objects, callback metadata, ordered events, ESP and nonvolatile
registers are checked. EAX is deliberately unused, matching the void caller.

Recorded command; immutable attempt001 is already complete:

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/386-heap-startup/native-config.json --attempt 1
```

Receipt: `local/experiments/v2-heap-startup-native-386/001/receipt.json`.
Source/config/fixture remain frozen. `evidence.json` pins the original bytes,
existing proofs, actual384 source closure and all11 production COFF definitions.
Canonical pool/page owner globals are linked directly; external files/list/disk
storage is explicitly fixture-controlled and checked against the real owners'
zero initialization before overrides. No original arena VA is treated as a host
pointer: the controlled VirtualAlloc endpoint returns distinct declared buffers.

Old305 pool shutdown and307 disk-table release are separately native-accepted
(48 cases each), and both unchanged sources compile in actual384. Their source,
summary, audit and proof-manifest hashes are reused. They are not compile-only
or invented replacements. They currently lack individual functions.json entries;
that registry gap does not erase their existing bounded proof. This fixture uses
fail-closed callback/release endpoints, registers the canonical declared shutdown
identity, and never executes those teardown bodies. Their composition is not
claimed by386.

The accepted domain starts with null devices/table/disk lock, empty lists and
zero pool roots; page cache is0 or4096 and all three allocations succeed. The
already-initialized fast path is checked only after such a successful call. This
does not prove that every earlier startup/reentry/failure path establishes those
assumptions. Reinitialization, registered-shutdown/CRT-registry composition,
allocation failure, invalid pages/counts/spans, concurrency and real OS/complete
heap/startup/game behavior remain open.

Expected closed linker name: `?application_heap_finish@porsche@@YAXIII@Z`.
The candidate COFF defines exactly that facade and refers only to the existing
canonical pool provider; there is no new dependency.
