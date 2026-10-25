# Open-Unleashed reference index

Source: [Second-Boot/Open-Unleashed](https://github.com/Second-Boot/Open-Unleashed),
commit `b0ec06e90c9885fca0f6afb6a2caa5aa02c0169a` (v0.1.4). This is an external
navigation aid. It is not accepted C++ behavior or an independent original-game oracle.

## Contents and validation

The [JSONL index](../../research/binary-index/supplemental/open-unleashed-crossref.jsonl)
contains 14,868 references from 244 files in the 272-file crate source scan.
All 14,868 extracted comment selections were checked against exact line spans
in the pinned source.
The [validation manifest](../../research/binary-index/supplemental/open-unleashed-crossref-validation.json)
records the source-file hashes, extraction identity and index hash.

Each record contains source provenance, file/line span, the first address candidate,
address/offset literals, a SHA-256 of the whitespace-normalized selected source
comment and the author's confidence/date marker. Full comment text stays in the
external source; the original extracted text is archived only under ignored local
experiments. No Rust implementation is imported into the game or relicensed here.

There are 12,668 address-bearing records (6,829 distinct primary addresses):
12,528 have a matching lexical reference in the local Porsche.exe catalog/listing;
140 do not. Another 2,200 records contain only offsets/constants. The flags were
independently reproduced using symbol/function/data addresses and listing operands.
A `local_index_reference_found` hit does not prove a function boundary, semantics,
or even that the literal was used as an address. False positives remain possible.

Some references concern dx7z.dll rather than Porsche.exe. Interior instruction
addresses and decoding gaps also explain misses. Keep those as tracing targets;
do not turn missing references into guessed function entries.

## Useful entry points

- Network defaults: UDP port 9442 near 005b4990 and the 0048e4cd connect path;
  IPX socket 1106 belongs to the00560b44/00560be1/00560e1c/00560e83 paths.
  Start at `nfs-net/src/lib.rs`.
- Frame and connection layers:00588910/00588a80,0055eae0 and0055d4a0 in
  `nfs-net/src/frame.rs`, `comm.rs`, `data.rs` and `outbox.rs`.
- Audio random state:00598ff0/005990c0 and005e347c..005e3490 in
  `nfs-audio/src/librand.rs`; impact/scrape producers0040fff0/0040fbc0 in `sfx.rs`.
- Front-end strings/accessors:004e06d0,005b4ce0 and00506600 in
  `open-unleashed/src/accessors.rs`.
- Career filters and opponent skill:00504d00/004ebda0 in `nfs-game/src/career.rs`
  and `ai.rs`.

These are source-author claims to check against our binary, callers and native
comparisons. They are not new accepted providers or additions to recovery counts.
The six current modem-frontier addresses0055df80/00584a60/00584bb7/00584dcd/
00584e72/0055e340 have no exact hits in this snapshot; use our original index.
See [the usage decision](open-unleashed-usage-2026-10-25.md) for workflow boundaries.
