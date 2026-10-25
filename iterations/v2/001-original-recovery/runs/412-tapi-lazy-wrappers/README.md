# Run412 — five original TAPI lazy wrappers

Recovered five complete Porsche.exe bodies (391B), following frozen Run408:

- `00584b70` / 71B: `modem_line_drop_00584b70`, stdcall3 / RET12.
- `00584bb7` / 78B: `modem_line_get_capabilities_00584bb7`, stdcall5 / RET20.
- `00584c4c` / 81B: `modem_line_get_id_00584c4c`, stdcall6 / RET24.
- `00584dcd` / 90B: `modem_tapi_line_open_00584dcd`, stdcall9 / RET36.
- `00584e72` / 71B: `modem_line_set_status_00584e72`, stdcall3 / RET12.

Production files: `tapi_lazy_wrappers.cpp` and `tapi_lazy_wrappers.hpp`.
The capabilities/open/status declarations match accepted Run410. The getID
contract matches candidate Run411: **output buffer fifth, class string sixth**.
Original `0055e8c0` pushes class `005bcb3c` then allocated ESI buffer at
`0055e8df/0055e8e4` before `0055e8f2 CALL`. Correcting the reversed pointer types
also corrects the C++ provider symbol; raw untyped argument transport alone did
not catch that error in the historical candidate.

Source: immutable `local/game/Porsche.exe`, SHA-256
`ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`.
`evidence.json` records index queries, callers, exact body/name hashes, RET tails
and relocation counts. Wrappers resolve a zero cache, mark it all-ones after a
resolver failure and return the first failure's exact bits; they reload after
success, return `0x80000049` for the sentinel, otherwise forward all arguments
and result bits to/from the cached stdcall target.

`ownership.json` checks the full current Run409 manifest **base plus delta**:
223 compiled roots, 394 code/header files, 555 total files. No prior owners of
these cache cells or names exist. New DWORD owners are `006ad0b8/15c/188/20c/2bc`;
all lie in the PE loader-zero-filled `.data` tail, asserted zero before native
fixture resets. Candidate Run411's caches `006ad048/068/0a0` are disjoint. No
surrounding array, capacity or original-VA host pointer mapping is invented.

## Verification

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/412-tapi-lazy-wrappers/native-config.json --attempt 5
```

Authoritative receipt:
`local/experiments/v2-tapi-lazy-wrappers-native-412/005/receipt.json` — PASS.
Each `/Od` and `/O2` variant compares 5,600 three-call sequences / 16,800 calls /
67,200 checks against complete original bodies. Covers empty/sentinel/both valid
cached targets; resolver and endpoint return bits; child cache writes and
post-resolution reloads; cross-cell writes; endpoint invalidation; every argument,
opaque pointers, backed aliases/writes, full state, event order and stdcall
cleanup/nonvolatile registers. The fixture jointly includes Run410/411 headers,
asserts the getID function-pointer type, and uses typed getID endpoints. All eight
mutations are detected: first error, failed cache, sentinel result, and argument
order for each of the five wrappers. All 173 proof pins match; code, config and
fixture are frozen as UTF-8 LF.

`coff-evidence.json` records actual `/Od` and `/O2` provider objects. The three
Run410 undefined symbols match exactly. Correct getID provider is
`?modem_line_get_id_00584c4c@porsche@@YGHIIIIPAXPBD@Z`; its declaration compiles
jointly with Run411. Actual shared game-object ownership/link checks belong to
coordinator Run413.

History is preserved: 001/002 failed mutation compilation; 003 passed raw
transport with the wrong typed getID contract (exact six files archived under
`historical-003-inputs`); corrected 004 passed, then the sole changed input was
Run411's scratch-span comment. Its exact prior header is hash-verified under
`historical-004-headers`; 005 pins the frozen final header. No Run412 code/config
changed after 004.

## External boundary

`tapi_resolve_00584a60(const char*, uint32_t*) -> int32_t __stdcall` is external
to this batch. Its real candidate provider exists at
`runs/411-modem-callback/modem_callback.cpp:54`; Run412's native fixture uses a
controlled resolver. Shared intake must use the real provider and its module
cache, never the fixture. Successful resolution to NULL is excluded because the
original then calls NULL. Live TAPI/modem devices and concurrent cache access
remain unvalidated. No registry, CMake, shared plan, Run411 or commit edits.
