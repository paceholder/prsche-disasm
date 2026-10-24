# Run394: original network clock import binding

Original0048a9a0 is the complete6B JMP[005b2080] thunk to KERNEL32!GetTickCount.
The existing network_lifecycle_tick_count declaration has the same stdcall0
ABI and DWORD return. This source closes the existing typed boundary.

Shared392 acceptance requires the emitted COFF body to be exactly the same
JMP form with a sole relocation to the same original DLL/import. No state,
clock approximation, native comparison count or new clock algorithm is added.
Raw PE/import evidence is in evidence.json. Live scheduling remains outside
this import-identity proof. The final COFF verification belongs to392.
