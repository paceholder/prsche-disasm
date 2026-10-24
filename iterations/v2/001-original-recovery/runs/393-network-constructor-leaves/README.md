# Run393: constructor word and missing-file callback

Recovers Porsche.exe0052df00 (9B, ECX receiver/EAX=self, clear exactly4B) and
0048d9b0 (57B, cdecl filename/error callback) using Run391's exact declarations.
The callback gates on the existing startup network pointer, full32-bit+8 word
and single+0xbf byte, then forwards sender and filename as borrowed raw tokens
to canonical0048c870 with all five original message arguments. No pointer
auto-dereference, address mapping, new state or child implementation is added.

Shared native001 **PASS /Od and /O2**: **10816 cases per variant** (16 word,
10800 callback), **54080 checks per variant, six detected mutations**.
Proof covers unaligned receiver writes and guard bytes, full object/canary
preservation, exact gates/argument bits, NULL and invalid filename/sender
tokens, signed error boundaries, child call snapshots and stack/nonvolatile
registers. Word return is checked exactly; callback EAX is intentionally
ignored because the original callback contract is void.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/393-network-constructor-leaves/native-config.json --attempt 001
```

Passed source/config/fixture frozen. Authoritative209-input receipt:
`local/experiments/v2-network-constructor-leaves-native-393/001/receipt.json`.
Evidence contains exact raw/index bodies, ABI, canonical provider identity
and shared390/002 callback REA hash. Two backend inspection candidates remain
local research artifacts; no new session, ISA corpus run or runtime entered
production. Run391's accepted inputs remain unchanged.

Production intake consists solely of `network_constructor_leaves.cpp`, which
references existing startup state and canonical network message provider.
The standalone fixture controls that state/callee; actual owner source and
earlier message proof are pinned, final COFF ownership belongs to coordinator392.
Word receiver needs writable4B, callback network needs readable fields through
+0xbf; original invalid-memory faults are outside the bounded proof. Live
packets/network and full startup remain open. The existing311 destructor
0048ff30 remains an unrecovered typed external.
