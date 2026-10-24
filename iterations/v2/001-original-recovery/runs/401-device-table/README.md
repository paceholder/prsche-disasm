# Run401: original modem table accessor and cleanup

Native003 PASS:1710 cases at each of `/Od` and `/O2`,3420 total;6840 checks
per variant,six detected mutations. Frozen129B:0055eb70 accessor46B and
0055ddf0 cleanup83B. REA003 confirms allthree original dossiers, including
522B initializer0055e4d0; shared REA audit PASS.

Run391's frozen `network_environment` facade is retained for its exact cdecl
uint32 ABI. The original accessor initializes first, even for a negative index,
then uses signed index/count bounds and wrapping12B address arithmetic. It does
not dereference the resulting record. Original modemBuf/tapiname labels and
lineInitialize/lineNegotiateAPIVersion/lineGetLineDevStatus wrappers identify the
modem/TAPI table; this is not a generic environment or display lookup.

Two new canonical scalar owners correspond to006a57a0/count and006a57b8/pointer.
Both are loader-zero-filled .data cells, beyond raw section bytes; evidence records
section extents rather than inventing file bytes. No prior accepted body or C++
state definition owns them. No backing-array capacity/storage is introduced.
Initializer0055e4d0 remains a real unresolved provider. Cleanup reuses canonical
free00531f90, reloads table after string frees and count at every iteration, frees
the resulting table and clears only the pointer. Count retention is intentional.

The fixture retains full original bodies, verifies relocation operands/calls and
compares returns, ordered child-visible state, final cells, complete bounded memory
and cdecl stack/nonvolatile ABI. Getter includes high-bit signed bounds and wrapping
nondereferenced pointer tokens. Cleanup includes NULL entries, changing counts and
table replacements during free. Enumeration and free are controlled child seams;
no claim of real TAPI, allocator, concurrent or full-game composition is made.

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/401-device-table/native-config.json --attempt 3
py -3 scripts/research/audit-v2-rea-session.py --config iterations/v2/001-original-recovery/runs/401-device-table/rea-queries.json --attempt 003
```

Authoritative native receipt:local/experiments/v2-device-table-native-401/003/receipt.json.
001 passed baselines but a detected mutation lacked flushed stdout; shared runner
rejected its absent marker.002 fixes fixture output only and passes the whole batch.
REA001 sandbox spawnEPERM is preserved;002 ran with explicit process permission.
Next: original522B modem initializer and actual TAPI wrappers/callback ownership,
using the existing table owners. No guessed enumeration or success stub.

Final003 native/REA proofs pin canonical UTF8 LF configurations.002 was valid for
its CRLF inputs; staging revealed normalization, so final proof was refreshed before
commit. Source/header semantics and original bytes were unchanged.
