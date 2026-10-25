# Need for Speed: Porsche Unleashed / Porsche 2000 ? source recovery

Reverse engineering and C/C++ reconstruction of **Need for Speed: Porsche Unleashed**
(PC, 2000), also released as **Need for Speed: Porsche 2000** and commonly called
**NFS5**. The goal is to recover the original game's behavior from its Windows x86
binaries, build a working game, document its architecture, then use that foundation
for a Rust port to modern native and web platforms.

**Work in progress: the recovered C++ game does not yet link into a playable executable.**
The repository contains verified subsystems, experimental compilation tools and an
older Rust/WebGPU prototype. These have separate acceptance criteria.

[Current status](STATUS.md) ? [Roadmap](PLAN.md) ? [Binary index](research/binary-index/README.md) ?
[Build instructions](docs/building.md) ? [Contributing](CONTRIBUTING.md)

## What is being recovered

The target is the original driving and game experience: vehicle physics and damage,
AI opponents and traffic, Quick Race, Evolution and Factory Driver careers, used-car
purchases, progression, saves, menus, rendering and audio.

Algorithms come from disassembly, decompiler analysis and original resource consumers.
Recovered functions are checked against original x86 behavior using recorded inputs,
intermediate state, outputs and ABI checks. Compilation alone is not fidelity proof.
Unresolved algorithms and unsupported domains remain explicit; a plausible substitute
is not accepted as original behavior.

## Project tracks

- **Active C/C++ recovery:** [v2 / 001-original-recovery](iterations/v2/001-original-recovery/README.md).
  The [active plan](iterations/v2/001-original-recovery/PLAN.md) owns subsystem scope,
  proof artifacts and the next dependency work. [STATUS.md](STATUS.md) records the latest
  measured build and its limitations.
- **Experimental Rust/WebGPU prototype:** [012-campaign-fidelity](iterations/012-campaign-fidelity/README.md).
  [PR #25 intake](docs/research/pr25-integration-review-2026-10-26.md) adds original-code
  recorders and vehicle, AI, audio and interface work. It remains unfinished, with
  explicit fidelity limits. Snapshots 001?011 are preserved.

The plan after a functional recovered baseline is an architecture map of the game,
followed by the Rust native/web port. There is no completion-date estimate.

## Build the recovered modules

Use Windows x64 with **Visual Studio 2022 Build Tools**, the C++ x86/x64 workload,
a Windows SDK, CMake and Python 3. From the repository root:

```powershell
./scripts/build-v2.ps1
```

This builds Win32 recovery modules and verification programs under
`local/builds/v2/001-original-recovery/`; it does not produce a complete playable game.
See [building.md](docs/building.md) for setup and the current full game-link check.

Supply your own original game files under `local/game/`. They are read-only inputs
and are not distributed with this project. [Original installation provenance](docs/game-build.md)
explains the reference copy and known patch boundaries. Generated builds, caches and
local experiments stay outside Git.

## Find code and evidence

- [Binary index](research/binary-index/README.md): start here for original modules,
  addresses, strings, imports, function references and coverage gaps.
- [Recovered source registry](iterations/v2/001-original-recovery/source/recovered/functions.json):
  accepted function owners and explicit partial/unresolved boundaries.
- [Recovery runs](iterations/v2/001-original-recovery/runs/): immutable per-batch sources,
  manifests, validation receipts and concise handoffs.
- [Shared proof manifests](research/v2/proof-manifests/README.md): reusable input hashes.
- [Validation backlog](docs/recovery-validation-backlog.md): outstanding original,
  integration, platform and visual checks.
- [Documentation index](docs/README.md): formats, tools, build setup and recovery notes.

Experimental x86-to-C++ lowering and compile candidates are research tools. Their
coverage counts are separate from accepted game functions and playable-game progress.

## Contributing

Original-code findings, reproducible fixtures, resource-format research and focused
fixes are welcome. Include the binary/resource identity, relevant addresses and the
comparison that supports a behavior change. Keep approximations and unverified claims
clearly identified. See [CONTRIBUTING.md](CONTRIBUTING.md) and [AGENTS.md](AGENTS.md).

## License and game assets

Project-owned original contributions use the [MIT License](LICENSE), subject to
[LICENSING.md](LICENSING.md). Original game binaries, resources, reproduced game
material and third-party contributions are not automatically relicensed. You need
your own copy of the game. This is an unofficial project, unaffiliated with Electronic
Arts, Eden Games or Porsche AG.

## Related work and acknowledgements

Resource research has benefited from [Denis Auroux's CRP/FSH tools](https://auroux.free.fr/),
[OpenNFS / LibOpenNFS](https://github.com/OpenNFS/LibOpenNFS) and
[dima424658 / LibOpenNFS](https://github.com/dima424658/LibOpenNFS).
[Open-Unleashed](https://github.com/Second-Boot/Open-Unleashed) is a separate Rust
reimplementation; evaluating it does not establish original-game fidelity or change
this project's licensing scope.
