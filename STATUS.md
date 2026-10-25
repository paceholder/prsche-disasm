# Current project status

Updated 2026-10-25. Active work: [v2 / 001-original-recovery](iterations/v2/001-original-recovery/PLAN.md).

## Recovered C/C++ game

[Run409](iterations/v2/001-original-recovery/runs/409-tapi-modem-integration/README.md)
compiles 223 C++ translation units across 8 projects. The game link still has 195 unresolved
names / 203 references. **There is no recovered playable C++ game executable yet.**

The latest batch integrates eight original TAPI/modem bodies after 21,580 native
cases/sequences, 14 detected mutations and a 555-pin audit. These are this batch's
results, not a project-wide completion percentage. Partial and unsupported domains,
including the modem enumerator's sparse/unwritten data, remain explicit.

Next original dependencies include callback0055df80, resolver00584a60, TAPI wrappers
00584bb7/00584dcd/00584e72 and configuration0055e340. Detailed evidence and the actual
next work order live in the [iteration PLAN](iterations/v2/001-original-recovery/PLAN.md).
Full startup, display, menus, careers and gameplay still need integration/acceptance.

## Other workstreams

The [Rust iteration 012 prototype](iterations/012-campaign-fidelity/README.md) is unfinished and
its integration is paused. Compile candidates and x86 lowering tools are research;
they are separate from accepted game code.

The independent Open-Unleashed v0.1.4 Windows release was tested with our assets in an
isolated local folder: Quick Race, garage rendering and first Factory Driver mission
all exited successfully and produced screenshots. Quick Race advanced 1,280 ticks;
the mission smoke run failed its objective and does not prove career completion.
Native Computer Use returned `spawn EPERM`, so no interactive visual acceptance is
claimed. Original asset sizes/mtimes were unchanged. This is external-port evidence,
not a successful build or fidelity proof of our C++ reconstruction.

[PR #25](https://github.com/xfynx/prsche-disasm/pull/25) is under review while its author
polishes the branch. Its original-function recorders and fixtures are useful for v2;
merging the Rust implementation and accepting original C++ behavior are separate checks.

## Evidence and remaining checks

Use the [binary index](research/binary-index/README.md), [recovery runs](iterations/v2/001-original-recovery/runs/),
[validation backlog](docs/recovery-validation-backlog.md) and [roadmap](PLAN.md).
Older checkpoints remain in the iteration history and Git; this file describes current state.
