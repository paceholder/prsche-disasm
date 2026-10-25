# Using Open-Unleashed research

The goal remains original C/C++ recovery, a documented architecture, then a Rust
port. Open-Unleashed is a separate Rust implementation under GPL-3.0-or-later.
The pinned review source is commit `b0ec06e90c9885fca0f6afb6a2caa5aa02c0169a`.

## Current decision

Use its address-annotated comments as navigation to original functions, fields,
constants and consumers. The [cross-reference summary](open-unleashed-crossref-summary.md)
and [metadata index](../../research/binary-index/supplemental/open-unleashed-crossref.jsonl)
make those references searchable without importing the Rust implementation.

An address, author confidence marker or agreement with our existing Rust code
is a lead. Confirm original bytes/ABI, ownership, callers and behavior before
accepting a C++ implementation. The original binary remains the oracle. In
particular, their self-regression tests do not replace original-native comparisons.
Unpublished `RE/notes` referenced by the source are not part of the public checkout.

The module structure can inform the later Rust architecture work. This closeout
imports no external implementation and makes no licensing change. Any future
code reuse must be evaluated separately with the applicable notices and project
licensing decision; the root MIT license does not relicense third-party material.

## Completed isolated trial

The user-authorized v0.1.4 Windows release was downloaded and verified against
its published SHA-256, then run with our game data and isolated saves/logs:

- Quick Race: 20 simulated seconds/1,280 ticks; about 256 m displacement, maximum 110 km/h.
- Garage: application-generated frame rendered successfully.
- Factory Driver: mission 0M01 loaded skidpad/_st1.scn and simulated 20 seconds.
  The automated drive failed the objective; it is not campaign-completion proof.

All three processes exited 0 and generated screenshots. The 1,742 original files'
sizes/mtimes were unchanged. The native Computer Use helper failed with `spawn EPERM`,
so screenshots and logs do not establish interactive UI acceptance or original
fidelity. Raw receipts/images remain in ignored
`local/experiments/open-unleashed-trial/v0.1.4`; no game artwork is published here.
Community-patch behavior reproduced by that port is a separate comparison boundary.

## Next use

Search the index for the current modem/network/audio/front-end dependency, follow
the pinned file/line link, then investigate our original index/REA and native oracle.
Do not rerun accepted native proof merely because an external comment agrees.

The cross-reference extraction and trial are complete. The GitHub MCP/metadata and
README cleanup have also been completed; they are not pending iteration milestones.
Asking the author for unpublished notes would be a separate communication requiring
user authorization. No outreach was sent as part of this work.
