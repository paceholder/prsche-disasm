---
name: porsche-recovery
description: Recover and integrate original Porsche Unleashed C/C++ behavior in this repository using the binary index, REA, native comparisons and shared batch audits. Use for recovery batches and their acceptance, not unrelated repository maintenance.
---

# Porsche original-code recovery

Run commands from the repository root. [AGENTS.md](../../../AGENTS.md) owns
project rules; the active iteration PLAN owns current scope and assignments.
This skill routes the repeatable procedure rather than duplicating those files.

## Select a batch

Read root PLAN/STATUS pointers, the active iteration PLAN and latest relevant run
README. Check `git status --short`; preserve unfinished work and immutable
attempts. Old historical assignments are not necessarily active.

Choose a coherent subsystem on the real startup/game dependency path. Use the
current frontier to locate missing providers, then trace callers for execution
order. Unresolved names are neither C++ files nor a completion estimate. Check
the active `source/recovered/functions.json` for canonical ownership, including
interior entries and body overlaps. Reuse proved providers through typed bindings
instead of recovering the same function or defining its state again. Delegate
independent work using [docs/agents.md](../../../docs/agents.md).

## Establish original evidence

Start at [the binary index](../../../research/binary-index/README.md). Search
strings/imports/resources, then inspect callers, body and callees. Resolve
decompiler ambiguities against assembly and raw PE bytes. Record module SHA,
VA/ranges, ABI, state ownership and data consumers in the batch evidence.
Investigate missing index coverage; keep unknown behavior explicit.

Reuse existing REA dossiers when sufficient. Otherwise batch related queries in
one manifest-driven session against an isolated original copy:

```powershell
py -3 scripts/research/run-v2-rea-session.py --config <rea-config> --attempt <NNN>
py -3 scripts/research/audit-v2-rea-session.py --config <rea-config> --attempt <NNN>
```

Inspect runner/config schemas only as needed. Extracted pseudocode from
`structure-v2.py` is research material, not accepted C++.

## Recover and compare

Use `scripts/research/x86-cpp-backend-exact.py` for mechanical C++ candidates or
instruction analysis when it saves manual work; see the bounded scope in
[Run431](../../../iterations/v2/001-original-recovery/runs/431-x86-backend-exact/README.md).
Keep rejected instructions explicit. Generated code does not establish ABI,
state ownership or original-function acceptance. Retain original-x86 comparisons
for each changed recovery batch. Reuse unchanged backend proof; do not rerun the
whole instruction census or add a bespoke runner for each function. CPU-specific
undefined flags, x87 precision and native-memory assumptions remain boundaries.

Reuse the shared native runner and nearest accepted subsystem fixture. Add the
new oracle/cases rather than a bespoke runner per function. Write new sources,
fixtures and configs as UTF-8 LF before proof; preserve numbered attempts:

```powershell
py -3 scripts/research/verify-v2-native-batch.py --config <native-config> --attempt <N>
```

Compare inputs, intermediate state, outputs, call order, used returns and ABI at
`/Od` and `/O2`, with mutations that detect material mistakes. Assert production
initial state before fixture resets. Verify raw original data before relocation
or pointer normalization so both sides cannot agree on an invented constant.
Preserve aliasing and failure behavior; explicitly bound opaque/uninitialized
state instead of silently zeroing it. Raw original VAs are not host pointers;
strings, callbacks and tables need proved transport. Label synthetic callsite
setup/RET instructions as harness code. Controlled COM/IAT endpoints establish
bounded transport, not live device acceptance.

Reuse unchanged accepted proof with matching source hashes. Repeat checks for
changed inputs, failures or a specific unresolved concern. After two failed
attempts on one hypothesis, report evidence and revise the approach.

## Integrate and accept

Freeze passed batches; place later work in the next batch. Add accepted sources
through the existing CMake chain. [Run363](../../../iterations/v2/001-original-recovery/runs/363-startup-bindings/README.md)
is a binding-manifest example; use the current baseline and measured results,
not its historical hashes or counts.

```powershell
py -3 scripts/research/build-v2-game-batch.py --config <build-config> --phase configure --attempt <NNN>
py -3 scripts/research/build-v2-game-batch.py --config <build-config> --phase build --attempt <NNN>
py -3 scripts/research/audit-v2-native-game-batch.py --config <acceptance-config>
```

Build/audit once per ready batch. Check compiler inputs, actual COFF ownership,
exact closed/new frontier names, original import identity and proof hashes.
Equal source basenames may require full relative object paths. No fixture/stub
providers belong in the game graph. Distinguish compilation, linked EXE, actual
startup/display and gameplay acceptance.

Keep one concise run handoff and update the active PLAN; root PLAN/STATUS/README
need brief pointers only. Record deferred domains and concrete next checks in
[the validation backlog](../../../docs/recovery-validation-backlog.md). Stage
explicit batch paths; inspect the diff and staged bytes against pinned inputs.
Commit/push within existing user authorization and preserve unrelated work.
