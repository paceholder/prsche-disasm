# Run404: three exact Win32 facades

Closes existing `file_set_last_error`, `startup_drive_type` and
`startup_create_process` contracts through original KERNEL32 SetLastError,
GetDriveTypeA and CreateProcessA imports. These are platform bindings:
**zero new original function/body count, no new state**.

The cdecl SetLastError facade explicitly calls the existing canonical stdcall
formatter_crt_set_last_error provider, compiled unchanged in the proof.
Anonymous file_disk.os_set_last_error and its mutable disk callback are not
reused for the original direct-IAT file consumer. The other two facades keep
their frozen stdcall signatures, exact ANSI APIs, ten CreateProcess arguments,
BOOL/integer return bits and borrowed pointer/alias behavior.

Shared native001 **PASS /Od and /O2**: **217 cases,868 checks per variant;
five detected mutations**. The fixture verifies and patches its own exact
three KERNEL32 IAT cells, then compares original six-byte CALL fragments with
actual compiled facades/canonical SetLastError provider. Argument forwarding
and outer RET instructions are explicitly harness code. Proof checks stack
cleanup/nonvolatile registers, arbitrary return and inheritance/flags bits,
NULL/invalid/borrowed pointers, partial writes on controlled failure and aliases
among command/startup/output buffers. **No process was launched.**

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config iterations/v2/001-original-recovery/runs/404-platform-facades/native-config.json --attempt 001
```

Source/config/fixture frozen. Authoritative175-input receipt:
`local/experiments/v2-platform-facades-native-404/001/receipt.json`.
Evidence independently parses the original PE imports and pins caller spans:
00592600/SetLastError005b21a4,004a5ca7/GetDriveTypeA005b2060,
004a5d01/CreateProcessA005b204c. Accepted consumer/provider sources and headers
are unchanged and pinned; whole consumers are not relabelled freshly verified.
The proof establishes import/ABI transport, not live drive/process behavior or
actual Windows failure semantics. Production intake is only
`platform_facades.cpp`; coordinator owns CMake, actual COFF/frontier and shared
integration acceptance. No old source, registry or plan was edited.
