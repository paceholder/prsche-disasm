# Run408 — five original TAPI lazy wrappers

Recovered five complete Porsche.exe bodies (352B), using the exact declarations
from frozen Run403 `modem_enumeration.hpp`:

- `00584ce0` / 78B: `modem_line_initialize_00584ce0`, stdcall5 / RET20.
- `00584d7c` / 81B: `modem_line_negotiate_00584d7c`, stdcall6 / RET24.
- `00584c9d` / 67B: `modem_line_status_00584c9d`, stdcall2 / RET8.
- `00584eb9` / 63B: `modem_line_shutdown_00584eb9`, stdcall1 / RET4.
- `00584af2` / 63B: `modem_line_close_00584af2`, stdcall1 / RET4.

Production files: `tapi_wrappers.cpp` and `tapi_wrappers.hpp`. No registry,
CMake, accepted source, or plan changes. Parent owns integration.

## Original evidence and state

Source is immutable `local/game/Porsche.exe`, SHA-256
`ddd748fdbe6d2030e31f9257a4e01852749460b6b58560a6b4a8559d3799ff39`.
Index searches used the five entries, resolver `00584a60`, cache VAs and name
VAs in the existing functions/calls index and raw disassembly. Run403 is the
startup consumer; its real line initialization, version negotiation, status,
shutdown and close calls require these exact declarations.

Each wrapper resolves only a zero cache, marks it all-ones on resolver failure,
and returns that first failure's exact nonzero bits. It reloads the cache after
successful resolution; an all-ones cache returns `0x80000049`. Otherwise it
forwards every argument to the cached stdcall target and returns all result
bits. Target-side cache changes survive the call.

The complete Run406 manifest **base plus delta** contains all 220 compiled roots
and 388 code/header files. Searching this closure and the current run sources
found no prior owners of the five cache cells or names. No registered function
provider covers these entries. `evidence.json` preserves the closure inventory,
raw resolver bytes/hash and original cache consumers. The five independently
owned DWORDs (`006ad078/194/1e0/204/2e8`) lie in the PE's zero-filled `.data`
tail; production initialization is checked before fixture resets. No surrounding
array, capacity, module-cache storage, or original-VA host mapping is invented.
The five exact ASCII export names are verified against a pinned original span
before oracle relocation.

## Native proof and remaining boundary

Shared runner command:

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/408-tapi-wrappers/native-config.json --attempt 1
```

Authoritative receipt:
`local/experiments/v2-tapi-wrappers-native-408/001/receipt.json` — PASS.
Each `/Od` and `/O2` variant compares 5,600 three-call sequences / 16,800 calls /
67,200 checks against the full original x86 bodies. Cases cover initially empty,
sentinel and both cached targets; arbitrary resolver and endpoint return bits;
failed resolution with different child writes; post-resolution reloads;
cross-cell changes; target-side invalidation; all arguments including opaque
pointer tokens and aliases; cache/memory snapshots, event order, ESP and all
nonvolatile registers. All six mutations fail: first error bits, failed cache
mark, sentinel error bits, initialize output argument, negotiate argument order,
and status handle. All 162 proof pins still match. Passed code/config/fixture
are frozen; registry growth is not an input pin.

`tapi_resolve_00584a60(const char*, uint32_t*) -> int32_t __stdcall` remains a
typed real unresolved child. Its original 75B body proves RET8, LoadLibraryA /
GetProcAddress behavior and the cache-pointer output, but this batch does not
implement it or own its module cache `006ad048`. Controlled native endpoints
prove wrapper transport; they are fixture-only. Successful resolution to NULL
is excluded because the original subsequently calls NULL. Live TAPI, actual
modem enumeration and concurrent cache access remain unvalidated. Next concrete
dependency is the original resolver and canonical ownership of its module cache.
