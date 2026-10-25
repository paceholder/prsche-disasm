# Contributing

The active work is original C/C++ recovery in
[iterations/v2/001-original-recovery](iterations/v2/001-original-recovery/PLAN.md).
The earlier Rust iteration 012 remains a separate, unfinished prototype. Please state
which track a contribution changes and read [AGENTS.md](AGENTS.md).

## Evidence for behavior changes

1. Start from the [binary index](research/binary-index/README.md). Record the original
   module and SHA-256, function addresses, callers/callees or resource fields.
2. Check existing function and shared-state owners across the current compiled source
   and header closure, including accepted run directories. Reuse canonical owners.
3. Recover the observed algorithm. Compare original inputs, intermediate state, outputs
   and ABI. State the tested domain, approximations and missing original behavior.
4. Reuse the shared batch tools and unchanged accepted proof. Include exact commands,
   input identities and results; keep generated logs/caches under ignored local paths.

A decompiler listing, successful build, self-regression test or plausible visual result
alone does not establish original fidelity. Keep unsupported domains and deferred
checks in the [validation backlog](docs/recovery-validation-backlog.md).

## Pull requests

Describe the concrete change, original evidence, validation and remaining boundaries.
Keep unrelated changes separate when practical. If several subsystems form one coherent
contribution, explain their relationship and provide a navigation summary. Mark a branch
as still being polished when it is not ready for merge.

Do not include original game files, credentials, machine-local configuration or generated
build directories. `local/game` is immutable; experiments write to `local/experiments`.
Snapshots 001?011 and accepted proof inputs are preserved. Documentation changes normally
need link and consistency checks rather than a fresh native game build.

Preserve third-party notices and read [LICENSING.md](LICENSING.md). The root license does
not grant rights over original game material or someone else's contributions.
