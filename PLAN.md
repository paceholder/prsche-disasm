# Project roadmap

The goal is a working reconstruction of Need for Speed: Porsche Unleashed / Porsche 2000
from original game code and resources, followed by an architecture map and a Rust port.
Original behavior, both careers and the original UI remain the acceptance target.

## Current work

The active iteration is [v2 / 001-original-recovery](iterations/v2/001-original-recovery/PLAN.md).
Its PLAN owns detailed scope, owners, evidence, commands and handoffs. Current measured
results are in [STATUS.md](STATUS.md); historical checkpoints remain in iteration runs
and Git history.

The presentation cleanup, external-port trial and pending recovery batches are
closed out. Next is the ready [PR #25](https://github.com/xfynx/prsche-disasm/pull/25):
refresh its head, review its changes and author attribution, and reuse useful
original-code evidence for C++. Then evaluate [xerj](https://github.com/xerj-org/xerj).
These follow-ups do not silently change recovery acceptance or licensing.

## Recovery milestones

1. Recover and integrate the original C/C++ startup and subsystem dependencies, using
   canonical state owners and bounded original-x86 comparisons.
2. Link a real Win32 game executable; validate startup, resource loading and display.
3. Validate menus, Quick Race, Evolution, Factory Driver, physics, damage, AI, progression,
   used-car purchases, saves and audio against the original. Close the
   [validation backlog](docs/recovery-validation-backlog.md).
4. Document how the complete game works at code level: module boundaries, state,
   lifetimes, call/data flows and platform dependencies.
5. Use that verified structure for the Rust native/web port.

The older [Rust iteration 012 prototype](iterations/012-campaign-fidelity/README.md) is unfinished
and integration is paused. Snapshots 001?011 remain immutable. New experimental paths do
not silently replace the original-recovery goal or count as full-game acceptance.

## Working rules

Use [AGENTS.md](AGENTS.md), the [recovery skill](.agents/skills/porsche-recovery/SKILL.md)
and existing shared tools. Prioritize functional dependencies; batch native validation,
reuse unchanged proof and delegate independent work. Keep `local/game` read-only.
