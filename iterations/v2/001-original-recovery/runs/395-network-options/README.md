# Run395: UDP/IPX initialization candidates

**Research candidate; no game intake.** Original005607d0 (108B) initializes
UDP;00560a90 (164B) starts WSA/IPX. Run391's frozen option-like names conceal
this behavior. These bodies contain no command-line parser or search. They
return zero or the transport descriptor's GUID address at+4; an optional
argument replaces its name only during successful first registration.

Shared native002 **PASS /Od and /O2**: **4608 mixed four-call sequences per
variant,18432 function calls and46080 checks per variant; six mutations**.
Proof compares complete raw bodies with actual C++ across existing/failed
locks, high-bit ready flags, UDP/WSA/open failures, repeated initialization,
NULL/invalid/borrowed name tokens, returns and ordered state/callback effects.
Late child changes test open/close slot replacement, registry index selection
after close, and the final flag reload after WSACleanup. Stack/nonvolatile
registers are checked; the400B WSADATA stack buffer remains opaque, with
controlled bounded writes and no uninitialized-content read.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/395-network-options/native-config.json --attempt 002
```

Source/config/fixture frozen after PASS. Authoritative170-input receipt:
`local/experiments/v2-network-options-native-395/002/receipt.json`.
Evidence records exact bodies, raw descriptors/GUIDs/HIGHLOW slots, registry
consumers, original WSOCK32 ordinal115/116 thunks and callback target bodies.
Backend lowering accepted both bodies as local inspection candidates only;
no new runner, REA session, ISA corpus run or production runtime was added.

Canonical thread_start_lock006a57d8 and heap_lock_create005321f0 are reused.
Eight unknown state views remain extern-only: two ready flags, registry count,
two registry bases, two transport descriptors and their shared context. No
storage or registry capacity is invented. Fixture indices0..7 are a bounded
domain, not an original capacity claim. One IPX descriptor view supplies both
indirect cells+0x1c/+0x20; independent interior cells are absent. Complete raw
targets00560840 (196B) and00587860 (131B) establish cdecl3/U return and cdecl1/U
return respectively, with no incoming ECX/EDX argument consumption. The latter
return is ignored. Other descriptor callback signatures remain unresolved.

Next trace is registry consumers005801b0/005802d0 and registration00582240,
then shared backing extents/ownership and exact consumer bindings for all19
callback slots in each96B descriptor. Until that closes, this package stays
outside game CMake/registry. Live WSA/network, concurrency, teardown/reinit
composition and full-game acceptance remain open. Accepted390/391/393 inputs
were not changed; no additional work was started after395 passed.

Final label review corrected the new IPX/UDP view names: descriptor005df6b0
points to UDP at005d07c0;005df710 points to IPX at005d07c4.001 had already
passed the same272B algorithms; exact superseded inputs are preserved under
`local/experiments/v2-network-options-native-395/historical-001-inputs` and
match its original hashes.002 independently passes all corrected inputs.
